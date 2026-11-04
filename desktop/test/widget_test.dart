import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:storage_bridge/main.dart';
import 'package:liquid_ui_components/liquid_ui_components.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  testWidgets('disconnected account shows setup and missing driver', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1280, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          native,
          (MethodCall call) async => jsonEncode({
            'version': 1,
            'ok': true,
            'result': {
              'connected': false,
              'configured': false,
              'pending': 0,
              'cache_bytes': 0,
              'cache_budget': 10737418240,
              'winfsp_installed': false,
            },
          }),
        );
    await tester.pumpWidget(const StorageBridgeApp());
    await tester.pumpAndSettle();
    expect(find.text('Connect your Google account'), findsOneWidget);
    expect(find.text(usesMacOS ? 'Get macFUSE' : 'Get WinFsp'), findsOneWidget);
    expect(find.text('Import OAuth client'), findsOneWidget);
    expect(find.byType(LiquidGlassNavigationBar), findsOneWidget);
    expect(find.byType(LiquidGlassButton), findsWidgets);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });
  testWidgets(
    'online-only switch persists through IPC and disables cache budget',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 1000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      var enabled = false;
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(native, (call) async {
            final request =
                jsonDecode(call.arguments as String) as Map<String, dynamic>;
            if (request['method'] == 'settings') {
              enabled = request['params']['online_only'] as bool;
            }
            return jsonEncode({
              'version': 1,
              'ok': true,
              'result': request['method'] == 'status'
                  ? {
                      'connected': false,
                      'online_only': enabled,
                      'streaming': true,
                      'read_cache_bytes': 0,
                      'read_cache_budget': 33554432,
                      'cache_bytes': 0,
                      'cache_budget': 10737418240,
                    }
                  : <String, dynamic>{},
            });
          });
      await tester.pumpWidget(const StorageBridgeApp());
      await tester.pumpAndSettle();
      await tester.tap(find.text('Settings'));
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<TextField>(find.byKey(const Key('retained-cache-budget')))
            .enabled,
        true,
      );
      expect(find.byType(LiquidGlassSwitch), findsNWidgets(2));
      await tester.tap(find.text('Online-only mode'));
      await tester.pumpAndSettle();
      expect(enabled, true);
      expect(
        tester
            .widget<TextField>(find.byKey(const Key('retained-cache-budget')))
            .enabled,
        false,
      );
      await tester.tap(find.text('Overview'));
      await tester.pumpAndSettle();
      expect(find.text('Temporary local storage'), findsOneWidget);
      expect(find.text('0.0 MB temporarily on this PC'), findsOneWidget);
      expect(find.text('Read buffer (RAM): 0.0 MB / 32.0 MB'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );

  testWidgets(
    'content allowance is editable in online-only mode and sent through IPC',
    (tester) async {
      tester.view.physicalSize = const Size(1280, 1100);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      var limit = 1073741824;
      var released = false;
      String? copiedReport;
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(SystemChannels.platform, (call) async {
            if (call.method == 'Clipboard.setData') {
              copiedReport = (call.arguments as Map)['text'] as String;
            }
            return null;
          });
      addTearDown(
        () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
            .setMockMethodCallHandler(SystemChannels.platform, null),
      );
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(native, (call) async {
            final request =
                jsonDecode(call.arguments as String) as Map<String, dynamic>;
            if (request['method'] == 'settings') {
              limit = request['params']['content_limit'] as int;
            }
            if (request['method'] == 'release_cache') released = true;
            return jsonEncode({
              'version': 1,
              'ok': true,
              'result': request['method'] == 'status'
                  ? {
                      'connected': false,
                      'online_only': true,
                      'streaming': true,
                      'read_cache_bytes': 33554432,
                      'read_cache_budget': 33554432,
                      'cache_bytes': 1048576,
                      'cache_budget': 10737418240,
                      'storage': {
                        'used_bytes': 1048576,
                        'limit_bytes': limit,
                        'available_bytes': limit - 1048576,
                        'pending_bytes': 1048576,
                        'retained_bytes': 0,
                        'recovery_bytes': 0,
                        'file_count': 1,
                        'write_blocked': true,
                        'over_limit': false,
                        'files': [
                          {
                            'name': 'pending.txt',
                            'bytes': 1048576,
                            'reason': 'pending_upload',
                          },
                        ],
                      },
                    }
                  : <String, dynamic>{},
            });
          });
      await tester.pumpWidget(const StorageBridgeApp());
      await tester.pumpAndSettle();
      expect(find.text('Your local storage budget'), findsOneWidget);
      await tester.ensureVisible(find.text('Why space is used'));
      await tester.tap(find.text('Why space is used'));
      await tester.pumpAndSettle();
      expect(find.text('pending.txt'), findsOneWidget);
      expect(find.text('Saved locally; awaiting upload'), findsOneWidget);
      await tester.ensureVisible(find.text('Free completed copies'));
      await tester.tap(find.text('Free completed copies'));
      await tester.pumpAndSettle();
      expect(released, true);
      await tester.ensureVisible(find.text('Copy storage report'));
      await tester.tap(find.text('Copy storage report'));
      await tester.pumpAndSettle();
      expect(jsonDecode(copiedReport!)['storage']['used_bytes'], 1048576);
      expect(copiedReport, isNot(contains('pending.txt')));
      await tester.ensureVisible(find.text('Settings'));
      await tester.tap(find.text('Settings'));
      await tester.pumpAndSettle();
      final input = find.byKey(const Key('content-limit'));
      expect(tester.widget<TextField>(input).enabled, true);
      await tester.ensureVisible(input);
      await tester.enterText(input, '2');
      await tester.ensureVisible(find.text('Save content limit'));
      await tester.tap(find.text('Save content limit'));
      await tester.pumpAndSettle();
      expect(limit, 2147483648);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
}
