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
    expect(find.text('Get WinFsp'), findsOneWidget);
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
      expect(tester.widget<TextField>(find.byType(TextField)).enabled, true);
      expect(find.byType(LiquidGlassSwitch), findsNWidgets(2));
      await tester.tap(find.text('Online-only mode'));
      await tester.pumpAndSettle();
      expect(enabled, true);
      expect(tester.widget<TextField>(find.byType(TextField)).enabled, false);
      await tester.tap(find.text('Overview'));
      await tester.pumpAndSettle();
      expect(find.text('Temporary local storage'), findsOneWidget);
      expect(find.text('0.0 MB temporarily on this PC'), findsOneWidget);
      expect(find.text('Read buffer (RAM): 0.0 MB / 32.0 MB'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
}
