import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:liquid_ui_components/liquid_ui_components.dart';

const native = MethodChannel('dev.storagebridge/desktop');
void main() => runApp(const StorageBridgeApp());

class StorageBridgeApp extends StatelessWidget {
  const StorageBridgeApp({super.key});
  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'Storage Bridge',
    debugShowCheckedModeBanner: false,
    theme: ThemeData(
      useMaterial3: true,
      fontFamily: 'Segoe UI',
      colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xff347a74)),
      scaffoldBackgroundColor: const Color(0xffedf3f4),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 19),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(18),
          ),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          padding: const EdgeInsets.symmetric(horizontal: 22, vertical: 19),
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(18),
          ),
          side: const BorderSide(color: Color(0xffabc9c4)),
        ),
      ),
      chipTheme: ChipThemeData(
        backgroundColor: Colors.white.withValues(alpha: .6),
        side: const BorderSide(color: Colors.white),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(30)),
        padding: const EdgeInsets.all(10),
      ),
      inputDecorationTheme: const InputDecorationTheme(
        border: OutlineInputBorder(),
      ),
    ),
    builder: (context, child) => LiquidGlassTheme(
      settings: LiquidGlassSettings(
        rendering: LiquidGlassRendering.balanced,
        tintColor: Colors.white,
        foregroundColor: const Color(0xff203e43),
        opacity: .48,
        blur: 18,
        dynamicsIntensity: MediaQuery.disableAnimationsOf(context) ? 0 : .35,
      ),
      child: child!,
    ),
    home: const Dashboard(),
  );
}

class Dashboard extends StatefulWidget {
  const Dashboard({super.key});
  @override
  State<Dashboard> createState() => _DashboardState();
}

class _DashboardState extends State<Dashboard> {
  Map<String, dynamic>? status;
  String? error;
  String letter = 'G:';
  int page = 0;
  bool busy = false, refreshing = false;
  Timer? timer;
  final cache = TextEditingController(text: '10');

  @override
  void initState() {
    super.initState();
    refresh();
    timer = Timer.periodic(const Duration(seconds: 2), (_) => refresh());
  }

  @override
  void dispose() {
    timer?.cancel();
    cache.dispose();
    super.dispose();
  }

  Future<Map<String, dynamic>> request(
    String method, [
    Map<String, dynamic>? params,
  ]) async {
    final raw = await native.invokeMethod<String>(
      'request',
      jsonEncode({'version': 1, 'method': method, 'params': params ?? {}}),
    );
    final reply = jsonDecode(raw!) as Map<String, dynamic>;
    if (reply['ok'] != true) {
      throw Exception(reply['error']);
    }
    return (reply['result'] as Map).cast<String, dynamic>();
  }

  Future<void> refresh() async {
    if (busy || refreshing) {
      return;
    }
    refreshing = true;
    try {
      final next = await request('status');
      if (mounted && jsonEncode(next) != jsonEncode(status)) {
        acceptStatus(next);
      }
    } catch (e) {
      if (mounted && status != null) {
        setState(() {
          status = null;
          error = 'Background engine disconnected. Start it again to reconnect; pending files remain on this PC.';
        });
      }
    } finally {
      refreshing = false;
    }
  }

  void acceptStatus(Map<String, dynamic> next) {
    setState(() {
      if (status == null) {
        letter = next['drive_letter'] as String? ?? 'G:';
        cache.text =
            ((next['cache_budget'] as num? ?? 10737418240) / 1073741824)
                .toString();
      }
      status = next;
    });
  }

  Future<void> act(Future<void> Function() work) async {
    setState(() {
      busy = true;
      error = null;
    });
    try {
      await work();
    } catch (e) {
      if (mounted) {
        setState(
          () => error = e is PlatformException
              ? e.message
              : e.toString().replaceFirst('Exception: ', ''),
        );
      }
    } finally {
      if (mounted) {
        setState(() => busy = false);
        await refresh();
      }
    }
  }

  Future<void> start(bool demo) => act(() async {
    await native.invokeMethod<void>('launch', demo);
    for (var i = 0; i < 25; i++) {
      await Future<void>.delayed(const Duration(milliseconds: 200));
      try {
        final next = await request('status');
        if (mounted) {
          acceptStatus(next);
        }
        return;
      } catch (_) {}
    }
    throw Exception(
      'Engine did not start. Check that storage-bridge.exe is beside the desktop app.',
    );
  });
  Future<void> importClient() => act(() async {
    final client = await native.invokeMethod<String>('chooseClient');
    if (client != null) {
      await request('configure', {'client_json': client});
    }
  });
  String bytes(dynamic n) {
    final v = (n as num? ?? 0).toDouble();
    return v >= 1073741824
        ? '${(v / 1073741824).toStringAsFixed(1)} GB'
        : '${(v / 1048576).toStringAsFixed(1)} MB';
  }

  @override
  Widget build(BuildContext context) {
    final s = status ?? {};
    final connected = s['connected'] == true;
    final mountedDrive = s['mounted'] == true;
    return Scaffold(
      body: AuroraBackdrop(
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            SizedBox(
              width: 244,
              child: Padding(
                padding: const EdgeInsets.fromLTRB(16, 16, 0, 16),
                child: LiquidGlassSurface(
                  settings: const LiquidGlassSettings(
                    rendering: LiquidGlassRendering.balanced,
                    tintColor: Color(0xff173e45),
                    foregroundColor: Colors.white,
                    opacity: .94,
                    blur: 22,
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Padding(
                        padding: const EdgeInsets.fromLTRB(24, 38, 16, 36),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Image.asset(
                              'assets/storage_bridge.png',
                              width: 64,
                              height: 64,
                              semanticLabel: 'Storage Bridge',
                            ),
                            SizedBox(height: 18),
                            Text(
                              'Storage Bridge',
                              style: TextStyle(
                                color: Colors.white,
                                fontSize: 21,
                                fontWeight: FontWeight.w700,
                              ),
                            ),
                            SizedBox(height: 7),
                            Text(
                              'YOUR CLOUD. CLOSER.',
                              style: TextStyle(
                                color: Color(0xff9baab4),
                                fontSize: 10,
                                letterSpacing: 2,
                              ),
                            ),
                          ],
                        ),
                      ),
                      const Spacer(),
                      const Padding(
                        padding: EdgeInsets.all(24),
                        child: Text(
                          'Windows preview · v0.3.0\nCustom Rust storage engine',
                          style: TextStyle(
                            color: Color(0xff9baab4),
                            height: 1.8,
                            fontSize: 11,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
            Expanded(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(32),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                [
                                  'Your cloud workspace',
                                  'Transfer queue',
                                  'Recent activity',
                                  'Bridge settings',
                                ][page],
                                style: const TextStyle(
                                  fontSize: 28,
                                  fontWeight: FontWeight.w700,
                                ),
                              ),
                              const SizedBox(height: 8),
                              const Text(
                                'Google Drive, right where you work.',
                                style: TextStyle(color: Color(0xff667581)),
                              ),
                            ],
                          ),
                        ),
                        LiquidGlassIconButton(
                          icon: const Icon(Icons.refresh_rounded, size: 20),
                          tooltip: 'Refresh bridge status',
                          onPressed: busy ? null : refresh,
                        ),
                        const SizedBox(width: 12),
                        Chip(
                          avatar: Icon(
                            connected
                                ? Icons.cloud_done_outlined
                                : Icons.cloud_off_outlined,
                            size: 17,
                          ),
                          label: Text(
                            s['demo'] == true
                                ? 'Local test mode'
                                : connected
                                ? 'Connected'
                                : 'Not connected',
                          ),
                        ),
                      ],
                    ),
                    const SizedBox(height: 24),
                    LiquidGlassNavigationBar(
                      selectedIndex: page,
                      onTap: (index) => setState(() => page = index),
                      shrinkOnScroll: false,
                      items: const [
                        LiquidGlassNavigationItem(
                          label: 'Overview',
                          icon: Icons.dashboard_outlined,
                        ),
                        LiquidGlassNavigationItem(
                          label: 'Transfers',
                          icon: Icons.swap_vert,
                        ),
                        LiquidGlassNavigationItem(
                          label: 'Activity',
                          icon: Icons.history,
                        ),
                        LiquidGlassNavigationItem(
                          label: 'Settings',
                          icon: Icons.tune,
                        ),
                      ],
                    ),
                    const SizedBox(height: 24),
                    if (page == 0) ...[
                      Wrap(
                        spacing: 14,
                        runSpacing: 14,
                        children: [
                          statusTile(
                            Icons.cloud_outlined,
                            'CONNECTION',
                            connected ? 'Cloud connected' : 'Ready to connect',
                          ),
                          statusTile(
                            Icons.folder_outlined,
                            'WINDOWS DRIVE',
                            mountedDrive
                                ? (s['drive_letter']?.toString() ?? letter)
                                : 'Not mounted',
                          ),
                          statusTile(
                            Icons.sync_rounded,
                            'UPLOAD QUEUE',
                            '${s['pending'] ?? 0} pending',
                          ),
                        ],
                      ),
                      const SizedBox(height: 24),
                    ],
                    if (error != null) banner(error!),
                    if (s['last_error'] != null)
                      banner(s['last_error'].toString()),
                    if (busy)
                      const Padding(
                        padding: EdgeInsets.only(bottom: 16),
                        child: LinearProgressIndicator(),
                      ),
                    if (status == null)
                      panel('Start your bridge', [
                        const Text(
                          'Start the background engine to connect Google Drive or try the local test drive.',
                        ),
                        const SizedBox(height: 20),
                        Wrap(
                          spacing: 12,
                          runSpacing: 12,
                          children: [
                            glassAction(
                              onPressed: busy ? null : () => start(false),
                              icon: const Icon(Icons.power_settings_new),
                              label: const Text('Start engine'),
                            ),
                            LiquidGlassButton(
                              onPressed: busy ? null : () => start(true),
                              child: const Text('Try local test drive'),
                            ),
                          ],
                        ),
                      ])
                    else if (page == 0) ...[
                      panel(
                        s['demo'] == true
                            ? 'Local filesystem test'
                            : 'Google Drive',
                        [
                          Row(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Container(
                                padding: const EdgeInsets.all(18),
                                decoration: BoxDecoration(
                                  color: const Color(0xffd5ebe5),
                                  borderRadius: BorderRadius.circular(16),
                                ),
                                child: const Icon(
                                  Icons.cloud_outlined,
                                  size: 42,
                                  color: Color(0xff347a74),
                                ),
                              ),
                              const SizedBox(width: 22),
                              Expanded(
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(
                                      s['about']?['user']?['displayName']
                                              ?.toString() ??
                                          'Connect your Google account',
                                      style: const TextStyle(
                                        fontSize: 21,
                                        fontWeight: FontWeight.w600,
                                      ),
                                    ),
                                    const SizedBox(height: 8),
                                    Text(
                                      s['about']?['user']?['emailAddress']
                                              ?.toString() ??
                                          'Read files directly from Drive. Edits upload in the background.',
                                    ),
                                    const SizedBox(height: 18),
                                    if (!connected)
                                      Wrap(
                                        spacing: 12,
                                        runSpacing: 10,
                                        children: [
                                          glassAction(
                                            onPressed: busy
                                                ? null
                                                : importClient,
                                            icon: const Icon(
                                              Icons.file_open_outlined,
                                            ),
                                            label: Text(
                                              s['configured'] == true
                                                  ? 'Replace OAuth client'
                                                  : 'Import OAuth client',
                                            ),
                                          ),
                                          LiquidGlassButton(
                                            onPressed:
                                                busy ||
                                                    s['configured'] != true ||
                                                    s['login_state'] ==
                                                        'waiting_for_browser'
                                                ? null
                                                : () => act(() async {
                                                    await request('login');
                                                  }),
                                            child: Text(
                                              s['login_state'] ==
                                                      'waiting_for_browser'
                                                  ? 'Waiting for browser…'
                                                  : 'Sign in with Google',
                                            ),
                                          ),
                                        ],
                                      ),
                                    if (connected)
                                      Text(
                                        '${s['pending'] ?? 0} pending uploads',
                                      ),
                                    if (connected && s['demo'] != true)
                                      TextButton(
                                        onPressed:
                                            busy ||
                                                s['login_state'] ==
                                                    'waiting_for_browser'
                                            ? null
                                            : () => act(() async {
                                                await request('login');
                                              }),
                                        child: const Text(
                                          'Reconnect Google account',
                                        ),
                                      ),
                                  ],
                                ),
                              ),
                            ],
                          ),
                          if (s['login_state'] != null &&
                              ![
                                'idle',
                                'connected',
                                'waiting_for_browser',
                              ].contains(s['login_state']))
                            Padding(
                              padding: const EdgeInsets.only(top: 16),
                              child: Text(
                                s['login_state'].toString(),
                                style: const TextStyle(color: Colors.red),
                              ),
                            ),
                        ],
                      ),
                      const SizedBox(height: 20),
                      panel('Your Windows drive', [
                        Text(
                          s['demo'] == true
                              ? 'Isolated local test drive — no Google connection.'
                              : 'Mounted folder: My Drive / StorageBridge',
                          style: const TextStyle(fontWeight: FontWeight.w600),
                        ),
                        const SizedBox(height: 10),
                        const Text(
                          'Browse files in File Explorer. Keep the engine running until uploads finish.',
                        ),
                        const SizedBox(height: 20),
                        Wrap(
                          spacing: 16,
                          runSpacing: 12,
                          crossAxisAlignment: WrapCrossAlignment.center,
                          children: [
                            SizedBox(
                              width: 130,
                              child: DropdownButtonFormField<String>(
                                initialValue: letter,
                                decoration: const InputDecoration(
                                  labelText: 'Drive letter',
                                ),
                                items: [
                                  for (var c = 68; c <= 90; c++)
                                    DropdownMenuItem(
                                      value: '${String.fromCharCode(c)}:',
                                      child: Text('${String.fromCharCode(c)}:'),
                                    ),
                                ],
                                onChanged: mountedDrive
                                    ? null
                                    : (v) => setState(() => letter = v!),
                              ),
                            ),
                            glassAction(
                              onPressed: busy || !connected
                                  ? null
                                  : () => act(() async {
                                      await request(
                                        mountedDrive ? 'unmount' : 'mount',
                                        {'letter': letter},
                                      );
                                    }),
                              icon: Icon(
                                mountedDrive ? Icons.eject : Icons.add_to_drive,
                              ),
                              label: Text(
                                mountedDrive ? 'Unmount drive' : 'Mount drive',
                              ),
                            ),
                            if (mountedDrive)
                              LiquidGlassButton(
                                onPressed: () => native.invokeMethod<void>(
                                  'openDrive',
                                  s['drive_letter'],
                                ),
                                child: const Text('Open File Explorer'),
                              ),
                          ],
                        ),
                        if (s['winfsp_installed'] != true)
                          Padding(
                            padding: const EdgeInsets.only(top: 16),
                            child: Wrap(
                              crossAxisAlignment: WrapCrossAlignment.center,
                              spacing: 12,
                              children: [
                                const Text(
                                  'WinFsp is required to mount a Windows drive.',
                                ),
                                TextButton(
                                  onPressed: () =>
                                      native.invokeMethod<void>('openWinfsp'),
                                  child: const Text('Get WinFsp'),
                                ),
                              ],
                            ),
                          ),
                      ]),
                      const SizedBox(height: 20),
                      panel(
                        s['online_only'] == true
                            ? 'Temporary local storage'
                            : 'Local cache',
                        [
                          Text(
                            s['online_only'] == true
                                ? '${bytes(s['cache_bytes'])} temporarily on this PC'
                                : '${bytes(s['cache_bytes'])} used / ${bytes(s['cache_budget'])} budget',
                            style: const TextStyle(
                              fontSize: 22,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          const SizedBox(height: 10),
                          if (s['streaming'] == true) ...[
                            Text(
                              'Read buffer (RAM): ${bytes(s['read_cache_bytes'])} / ${bytes(s['read_cache_budget'])}',
                            ),
                            const SizedBox(height: 8),
                          ],
                          Text(
                            s['streaming'] == true
                                ? 'Reads stream into RAM. Only new or changed data is staged on disk until upload finishes.'
                                : s['online_only'] == true
                                ? 'Online-only: local copies are removed after files close and uploads finish. Reopening requires internet.'
                                : 'Clean files can be evicted. Pending uploads and open files are retained.',
                          ),
                        ],
                      ),
                    ] else if (page == 1)
                      panel('Saved locally → uploaded', [
                        const Text(
                          'Wait for pending uploads to finish before switching computers.',
                        ),
                        const SizedBox(height: 16),
                        if ((s['transfers'] as List? ?? []).isEmpty)
                          const ListTile(
                            leading: Icon(
                              Icons.check_circle_outline,
                              color: Colors.green,
                            ),
                            title: Text('No pending uploads'),
                          ),
                        if (s['transfer'] != null) ...[
                          Text(
                            '${s['transfer']['direction']}: ${s['transfer']['name']}',
                          ),
                          const SizedBox(height: 8),
                          LinearProgressIndicator(
                            value: (s['transfer']['total'] as num? ?? 0) > 0
                                ? (s['transfer']['bytes'] as num) /
                                      (s['transfer']['total'] as num)
                                : null,
                          ),
                          const SizedBox(height: 12),
                        ],
                        for (final t in s['transfers'] as List? ?? [])
                          ListTile(
                            leading: const Icon(Icons.upload_file_outlined),
                            title: Text(t['name'].toString()),
                            subtitle: Text(
                              t['error']?.toString() ??
                                  t['state'].toString().replaceAll('_', ' '),
                            ),
                            trailing: Text('${t['attempts']} retries'),
                          ),
                        glassAction(
                          onPressed: busy
                              ? null
                              : () => act(() async {
                                  await request('retry');
                                }),
                          icon: const Icon(Icons.refresh),
                          label: const Text('Retry pending transfers'),
                        ),
                      ])
                    else if (page == 2)
                      panel('Activity', [
                        if ((s['events'] as List? ?? []).isEmpty)
                          const Text(
                            'Completed transfers and recovery events will appear here.',
                          ),
                        for (final event in s['events'] as List? ?? [])
                          ListTile(
                            leading: const Icon(Icons.check_circle_outline),
                            title: Text(event['message'].toString()),
                            subtitle: Text(
                              DateTime.fromMillisecondsSinceEpoch(
                                (event['time'] as int) * 1000,
                              ).toLocal().toString(),
                            ),
                          ),
                      ])
                    else
                      panel('Storage and startup', [
                        GlassSwitchTile(
                          contentPadding: EdgeInsets.zero,
                          title: const Text('Online-only mode'),
                          subtitle: Text(
                            s.containsKey('online_only')
                                ? 'Remove completed write copies after files close and uploads finish. Reads stream from Drive; pending edits stay protected on this PC.'
                                : 'Restart with the updated background engine to use online-only mode.',
                          ),
                          value: s['online_only'] == true,
                          onChanged: busy || !s.containsKey('online_only')
                              ? null
                              : (enabled) => act(() async {
                                  await request('settings', {
                                    'online_only': enabled,
                                  });
                                }),
                        ),
                        const Text(
                          'Settings apply to this PC. Open files and pending uploads are never evicted.',
                        ),
                        const SizedBox(height: 22),
                        SizedBox(
                          width: 240,
                          child: TextField(
                            enabled: !busy && s['online_only'] != true,
                            controller: cache,
                            keyboardType: TextInputType.number,
                            decoration: const InputDecoration(
                              labelText: 'Maximum cache (GiB)',
                            ),
                          ),
                        ),
                        const SizedBox(height: 12),
                        LiquidGlassButton(
                          onPressed: busy || s['online_only'] == true
                              ? null
                              : () => act(() async {
                                  final value = double.tryParse(cache.text);
                                  if (value == null ||
                                      !value.isFinite ||
                                      value < .0625 ||
                                      value > 16384) {
                                    throw Exception(
                                      'Enter a budget between 0.0625 and 16384 GiB.',
                                    );
                                  }
                                  await request('settings', {
                                    'cache_budget': (value * 1073741824)
                                        .round(),
                                  });
                                }),
                          child: const Text('Save cache limit'),
                        ),
                        GlassSwitchTile(
                          contentPadding: EdgeInsets.zero,
                          title: const Text(
                            'Reconnect when I sign in to Windows',
                          ),
                          subtitle: const Text(
                            'Start the engine and mount the last selected drive.',
                          ),
                          value: s['startup'] == true,
                          onChanged: busy
                              ? null
                              : (v) => act(() async {
                                  await request('settings', {'startup': v});
                                }),
                        ),
                        const Divider(height: 36),
                        const Text(
                          'Closing this window keeps the app in the tray. Quitting the UI leaves the background engine running.',
                        ),
                        const SizedBox(height: 16),
                        LiquidGlassButton(
                          onPressed: busy
                              ? null
                              : () => act(() async {
                                  await request('shutdown');
                                  if (mounted) {
                                    setState(() => status = null);
                                  }
                                }),
                          child: const Text('Stop background engine'),
                        ),
                      ]),
                    const SizedBox(height: 26),
                    Text(
                      s['streaming'] == true
                          ? 'Cloud streaming · Pending edits are staged locally until uploaded'
                          : s['online_only'] == true
                          ? 'Personal preview · Online-only · Temporary disk space required while files are in use'
                          : 'Personal preview · One Google account · Cached files available offline',
                      style: const TextStyle(
                        color: Color(0xff7c8790),
                        fontSize: 12,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget glassAction({
    required VoidCallback? onPressed,
    required Widget icon,
    required Widget label,
  }) => LiquidGlassButton(
    onPressed: onPressed,
    child: Row(
      mainAxisSize: MainAxisSize.min,
      children: [icon, const SizedBox(width: 10), label],
    ),
  );

  Widget statusTile(IconData icon, String label, String value) => SizedBox(
    width: 220,
    child: LiquidGlassSurface(
      padding: const EdgeInsets.all(20),
      child: Row(
        children: [
          Icon(icon, color: const Color(0xff347a74)),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  label,
                  style: const TextStyle(
                    fontSize: 10,
                    letterSpacing: 1.4,
                    color: Color(0xff637c80),
                    fontWeight: FontWeight.w600,
                  ),
                ),
                const SizedBox(height: 6),
                Text(
                  value,
                  style: const TextStyle(
                    fontSize: 16,
                    fontWeight: FontWeight.w600,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    ),
  );
  Widget panel(String title, List<Widget> children) => SizedBox(
    width: double.infinity,
    child: LiquidGlassSurface(
      borderRadius: BorderRadius.circular(28),
      padding: const EdgeInsets.all(26),
      child: Material(
        color: Colors.transparent,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              title,
              style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 18),
            ...children,
          ],
        ),
      ),
    ),
  );
  Widget banner(String message) => Container(
    width: double.infinity,
    margin: const EdgeInsets.only(bottom: 16),
    padding: const EdgeInsets.all(16),
    decoration: BoxDecoration(
      color: Colors.orange.shade50,
      borderRadius: BorderRadius.circular(10),
    ),
    child: SelectableText(
      message,
      style: TextStyle(color: Colors.orange.shade900),
    ),
  );
}

/// Static light fields avoid continuous idle animation.
class AuroraBackdrop extends StatelessWidget {
  const AuroraBackdrop({super.key, required this.child});
  final Widget child;
  @override
  Widget build(BuildContext context) => DecoratedBox(
    decoration: const BoxDecoration(
      gradient: LinearGradient(
        begin: Alignment.topLeft,
        end: Alignment.bottomRight,
        colors: [Color(0xffe2eeeb), Color(0xffeff1f7), Color(0xffe6eff0)],
      ),
    ),
    child: Stack(
      children: [
        Positioned(top: -180, right: -70, child: glow(const Color(0xffaadbd2))),
        Positioned(
          bottom: -220,
          left: 120,
          child: glow(const Color(0xffc6c8ea)),
        ),
        Positioned.fill(child: child),
      ],
    ),
  );
  Widget glow(Color color) => IgnorePointer(
    child: Container(
      width: 650,
      height: 650,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        gradient: RadialGradient(
          colors: [color.withValues(alpha: .65), color.withValues(alpha: 0)],
        ),
      ),
    ),
  );
}

class GlassSwitchTile extends StatelessWidget {
  const GlassSwitchTile({
    super.key,
    required this.title,
    required this.subtitle,
    required this.value,
    required this.onChanged,
    this.contentPadding,
  });
  final Text title;
  final Widget subtitle;
  final bool value;
  final ValueChanged<bool>? onChanged;
  final EdgeInsetsGeometry? contentPadding;
  @override
  Widget build(BuildContext context) => ListTile(
    contentPadding: contentPadding,
    title: title,
    subtitle: subtitle,
    onTap: onChanged == null ? null : () => onChanged!(!value),
    trailing: LiquidGlassSwitch(
      value: value,
      onChanged: onChanged,
      semanticLabel: title.data ?? 'Setting',
    ),
  );
}
