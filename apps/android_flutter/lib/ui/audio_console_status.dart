import 'package:flutter/material.dart';

import 'audio_console_theme.dart';

enum ConsoleUiState {
  idle,
  connecting,
  buffering,
  streaming,
  error,
}

class ConsoleStatusViewData {
  const ConsoleStatusViewData({
    required this.state,
    required this.titleEn,
    required this.titleZh,
    required this.accent,
    required this.background,
    required this.border,
  });

  final ConsoleUiState state;
  final String titleEn;
  final String titleZh;
  final Color accent;
  final Color background;
  final Color border;
}

class ConsoleStatusMapper {
  static const Set<String> _connectingStates = <String>{
    'handshaking',
    'negotiated',
    'connecting',
  };

  static const Set<String> _bufferingStates = <String>{
    'reconfiguring',
    'recovering',
    'buffering',
  };

  static ConsoleUiState map({
    required bool isConnecting,
    required bool wsConnected,
    required bool isPlaying,
    required bool isBuffering,
    required String runtimeState,
    required bool hasError,
  }) {
    final normalized = runtimeState.toLowerCase();
    if (hasError) return ConsoleUiState.error;
    if (isConnecting || _connectingStates.contains(normalized)) {
      return ConsoleUiState.connecting;
    }
    if (normalized == 'streaming' || (wsConnected && isPlaying)) {
      return ConsoleUiState.streaming;
    }
    if (_bufferingStates.contains(normalized) || isBuffering) {
      return ConsoleUiState.buffering;
    }
    return ConsoleUiState.idle;
  }

  static ConsoleStatusViewData viewData(ConsoleUiState state) {
    switch (state) {
      case ConsoleUiState.streaming:
        return const ConsoleStatusViewData(
          state: ConsoleUiState.streaming,
          titleEn: 'Streaming',
          titleZh: '推流中',
          accent: AudioConsoleColors.teal,
          background: AudioConsoleColors.tealDim,
          border: Color.fromRGBO(0, 212, 170, 0.32),
        );
      case ConsoleUiState.connecting:
        return const ConsoleStatusViewData(
          state: ConsoleUiState.connecting,
          titleEn: 'Connecting',
          titleZh: '连接中',
          accent: AudioConsoleColors.amber,
          background: Color.fromRGBO(245, 158, 11, 0.12),
          border: Color.fromRGBO(245, 158, 11, 0.3),
        );
      case ConsoleUiState.buffering:
        return const ConsoleStatusViewData(
          state: ConsoleUiState.buffering,
          titleEn: 'Buffering',
          titleZh: '缓冲中',
          accent: AudioConsoleColors.teal,
          background: Color.fromRGBO(0, 212, 170, 0.08),
          border: Color.fromRGBO(0, 212, 170, 0.2),
        );
      case ConsoleUiState.error:
        return const ConsoleStatusViewData(
          state: ConsoleUiState.error,
          titleEn: 'Error',
          titleZh: '异常',
          accent: AudioConsoleColors.error,
          background: Color.fromRGBO(239, 68, 68, 0.12),
          border: Color.fromRGBO(239, 68, 68, 0.3),
        );
      case ConsoleUiState.idle:
        return const ConsoleStatusViewData(
          state: ConsoleUiState.idle,
          titleEn: 'Idle',
          titleZh: '空闲',
          accent: AudioConsoleColors.text2,
          background: AudioConsoleColors.surface,
          border: AudioConsoleColors.borderStrong,
        );
    }
  }
}
