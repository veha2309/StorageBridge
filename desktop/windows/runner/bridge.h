#pragma once
#include <windows.h>
#include <flutter/method_channel.h>
#include <flutter/standard_method_codec.h>
#include <condition_variable>
#include <deque>
#include <mutex>
#include <thread>

// IPC runs on one worker. Flutter results are delivered on the window thread.
class DesktopBridge {
 public:
  static constexpr UINT kReply = WM_APP + 51;
  DesktopBridge(flutter::BinaryMessenger* messenger, HWND window);
  ~DesktopBridge();
  void Drain();
 private:
  using Result = flutter::MethodResult<flutter::EncodableValue>;
  struct Work { std::string payload; std::unique_ptr<Result> result; };
  struct Reply { std::string body; std::string error; std::unique_ptr<Result> result; };
  void Run();
  HWND window_;
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>> channel_;
  std::mutex mutex_;
  std::condition_variable changed_;
  std::deque<Work> work_;
  std::deque<Reply> replies_;
  bool stopping_ = false;
  std::thread worker_;
};
