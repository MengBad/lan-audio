import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android_mvp/ui/audio_console_status.dart';

void main() {
  test('maps to streaming when runtime state is streaming', () {
    final state = ConsoleStatusMapper.map(
      isConnecting: false,
      wsConnected: true,
      isPlaying: true,
      isBuffering: false,
      runtimeState: 'streaming',
      hasError: false,
    );
    expect(state, ConsoleUiState.streaming);
  });

  test('maps to connecting while handshake is in progress', () {
    final state = ConsoleStatusMapper.map(
      isConnecting: false,
      wsConnected: true,
      isPlaying: false,
      isBuffering: true,
      runtimeState: 'handshaking',
      hasError: false,
    );
    expect(state, ConsoleUiState.connecting);
  });

  test('maps to buffering for runtime recovering', () {
    final state = ConsoleStatusMapper.map(
      isConnecting: false,
      wsConnected: true,
      isPlaying: false,
      isBuffering: true,
      runtimeState: 'recovering',
      hasError: false,
    );
    expect(state, ConsoleUiState.buffering);
  });

  test('mode switch recovery maps back to streaming when playback resumes', () {
    final state = ConsoleStatusMapper.map(
      isConnecting: false,
      wsConnected: true,
      isPlaying: true,
      isBuffering: true,
      runtimeState: 'reconfiguring',
      hasError: false,
    );
    expect(state, ConsoleUiState.streaming);
  });

  test('maps to error when ui has explicit error', () {
    final state = ConsoleStatusMapper.map(
      isConnecting: false,
      wsConnected: false,
      isPlaying: false,
      isBuffering: false,
      runtimeState: 'disconnected',
      hasError: true,
    );
    expect(state, ConsoleUiState.error);
  });

  test('maps to idle when disconnected and no error', () {
    final state = ConsoleStatusMapper.map(
      isConnecting: false,
      wsConnected: false,
      isPlaying: false,
      isBuffering: false,
      runtimeState: 'disconnected',
      hasError: false,
    );
    expect(state, ConsoleUiState.idle);
  });
}
