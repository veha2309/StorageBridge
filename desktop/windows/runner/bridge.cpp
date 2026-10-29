#include "bridge.h"
#include <sddl.h>
#include <commdlg.h>
#include <shellapi.h>
#include <filesystem>
#include <fstream>
#include <stdexcept>
#include <vector>

namespace {
std::wstring PipeName() {
  HANDLE token = nullptr;
  if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token)) throw std::runtime_error("Cannot read current user");
  DWORD size = 0;
  GetTokenInformation(token, TokenUser, nullptr, 0, &size);
  std::vector<BYTE> bytes(size);
  bool ok = GetTokenInformation(token, TokenUser, bytes.data(), size, &size) != 0;
  CloseHandle(token);
  if (!ok) throw std::runtime_error("Cannot read current user SID");
  LPWSTR sid = nullptr;
  if (!ConvertSidToStringSidW(reinterpret_cast<TOKEN_USER*>(bytes.data())->User.Sid, &sid)) throw std::runtime_error("Cannot encode SID");
  std::wstring name = L"\\\\.\\pipe\\StorageBridge-v1-" + std::wstring(sid);
  LocalFree(sid);
  return name;
}
void Transfer(HANDLE pipe, void* data, DWORD length, bool write) {
  auto cursor = static_cast<BYTE*>(data);
  while (length) {
    OVERLAPPED overlapped{};
    overlapped.hEvent = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    if (!overlapped.hEvent) throw std::runtime_error("Cannot create IPC event");
    DWORD count = 0;
    BOOL ok = write ? WriteFile(pipe, cursor, length, &count, &overlapped) : ReadFile(pipe, cursor, length, &count, &overlapped);
    if (!ok && GetLastError() == ERROR_IO_PENDING) {
      if (WaitForSingleObject(overlapped.hEvent, 180000) == WAIT_OBJECT_0) {
        ok = GetOverlappedResult(pipe, &overlapped, &count, FALSE);
      } else {
        CancelIoEx(pipe, &overlapped);
        GetOverlappedResult(pipe, &overlapped, &count, TRUE);
        CloseHandle(overlapped.hEvent);
        throw std::runtime_error("Engine request timed out; pending writes remain on disk");
      }
    }
    CloseHandle(overlapped.hEvent);
    if (!ok || !count) throw std::runtime_error("Engine disconnected");
    cursor += count;
    length -= count;
  }
}
std::string Exchange(const std::string& payload) {
  if (payload.empty() || payload.size() > 1024 * 1024) throw std::runtime_error("Invalid IPC request size");
  auto name = PipeName();
  HANDLE pipe = CreateFileW(name.c_str(), GENERIC_READ | GENERIC_WRITE, 0, nullptr, OPEN_EXISTING, FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION, nullptr);
  for (int attempt = 0; pipe == INVALID_HANDLE_VALUE && attempt < 3; ++attempt) {
    DWORD code = GetLastError();
    if (code != ERROR_PIPE_BUSY && code != ERROR_FILE_NOT_FOUND) break;
    if (code == ERROR_PIPE_BUSY) WaitNamedPipeW(name.c_str(), 1000);
    else Sleep(10);
    pipe = CreateFileW(name.c_str(), GENERIC_READ | GENERIC_WRITE, 0, nullptr, OPEN_EXISTING, FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION, nullptr);
  }
  if (pipe == INVALID_HANDLE_VALUE) throw std::runtime_error("Background engine is unavailable. Start the engine to continue.");
  try {
    DWORD length = static_cast<DWORD>(payload.size());
    Transfer(pipe, &length, 4, true);
    Transfer(pipe, const_cast<char*>(payload.data()), length, true);
    Transfer(pipe, &length, 4, false);
    if (!length || length > 1024 * 1024) throw std::runtime_error("Invalid engine response size");
    std::string body(length, '\0');
    Transfer(pipe, body.data(), length, false);
    CloseHandle(pipe);
    return body;
  } catch (...) { CloseHandle(pipe); throw; }
}
void Launch(bool demo) {
  wchar_t path[32768];
  GetModuleFileNameW(nullptr, path, 32768);
  auto engine = std::filesystem::path(path).parent_path() / L"storage-bridge.exe";
  std::wstring command = L"\"" + engine.wstring() + L"\" --daemon" + (demo ? L" --demo" : L"");
  STARTUPINFOW startup{}; startup.cb = sizeof(startup);
  startup.dwFlags = STARTF_USESHOWWINDOW; startup.wShowWindow = SW_HIDE;
  PROCESS_INFORMATION process{};
  if (!CreateProcessW(engine.c_str(), command.data(), nullptr, nullptr, FALSE, CREATE_NO_WINDOW, nullptr, engine.parent_path().c_str(), &startup, &process)) throw std::runtime_error("Cannot start storage-bridge.exe. Build or reinstall the app.");
  CloseHandle(process.hProcess); CloseHandle(process.hThread);
}
}

DesktopBridge::DesktopBridge(flutter::BinaryMessenger* messenger, HWND window) : window_(window) {
  channel_ = std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(messenger, "dev.storagebridge/desktop", &flutter::StandardMethodCodec::GetInstance());
  channel_->SetMethodCallHandler([this](const auto& call, auto result) {
    try {
      if (call.method_name() == "request") {
        if (!call.arguments() || !std::holds_alternative<std::string>(*call.arguments())) throw std::runtime_error("Expected JSON request");
        { std::lock_guard<std::mutex> guard(mutex_); work_.push_back({std::get<std::string>(*call.arguments()), std::move(result)}); }
        changed_.notify_one(); return;
      }
      if (call.method_name() == "launch") {
        Launch(call.arguments() && std::holds_alternative<bool>(*call.arguments()) && std::get<bool>(*call.arguments()));
      } else if (call.method_name() == "chooseClient") {
        wchar_t file[32768]{};
        OPENFILENAMEW dialog{}; dialog.lStructSize = sizeof(dialog); dialog.hwndOwner = window_; dialog.lpstrFile = file; dialog.nMaxFile = 32768;
        dialog.lpstrFilter = L"Google OAuth JSON\0*.json\0\0"; dialog.Flags = OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR;
        if (!GetOpenFileNameW(&dialog)) { result->Success(); return; }
        std::ifstream input(std::filesystem::path(file), std::ios::binary | std::ios::ate);
        if (!input || input.tellg() > 65536) throw std::runtime_error("OAuth client JSON must be smaller than 64 KiB");
        input.seekg(0); std::string body((std::istreambuf_iterator<char>(input)), {});
        result->Success(flutter::EncodableValue(body)); return;
      } else if (call.method_name() == "openWinfsp") {
        ShellExecuteW(window_, L"open", L"https://winfsp.dev/rel/", nullptr, nullptr, SW_SHOWNORMAL);
      } else if (call.method_name() == "openDrive") {
        auto letter = std::get<std::string>(*call.arguments());
        if (letter.size() != 2 || letter[0] < 'D' || letter[0] > 'Z' || letter[1] != ':') throw std::runtime_error("Invalid drive letter");
        std::wstring target(letter.begin(), letter.end()); target += L"\\";
        ShellExecuteW(window_, L"open", target.c_str(), nullptr, nullptr, SW_SHOWNORMAL);
      } else { result->NotImplemented(); return; }
      result->Success();
    } catch (const std::exception& e) { if (result) result->Error("bridge", e.what()); }
  });
  worker_ = std::thread([this] { Run(); });
}
DesktopBridge::~DesktopBridge() {
  channel_->SetMethodCallHandler(nullptr);
  { std::lock_guard<std::mutex> guard(mutex_); stopping_ = true; }
  changed_.notify_one(); worker_.join(); Drain();
}
void DesktopBridge::Run() {
  while (true) {
    Work work;
    { std::unique_lock<std::mutex> guard(mutex_); changed_.wait(guard, [this] { return stopping_ || !work_.empty(); }); if (stopping_) return; work = std::move(work_.front()); work_.pop_front(); }
    Reply reply; reply.result = std::move(work.result);
    try { reply.body = Exchange(work.payload); } catch (const std::exception& e) { reply.error = e.what(); }
    { std::lock_guard<std::mutex> guard(mutex_); replies_.push_back(std::move(reply)); }
    PostMessageW(window_, kReply, 0, 0);
  }
}
void DesktopBridge::Drain() {
  std::deque<Reply> replies;
  { std::lock_guard<std::mutex> guard(mutex_); replies.swap(replies_); }
  for (auto& reply : replies) { if (reply.error.empty()) reply.result->Success(flutter::EncodableValue(reply.body)); else reply.result->Error("engine", reply.error); }
}
