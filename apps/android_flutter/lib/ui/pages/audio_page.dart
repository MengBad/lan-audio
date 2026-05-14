import 'package:flutter/material.dart';

import '../../services/mic_capture_service.dart';
import '../jitter_graph_widget.dart';
import '../mic_status_widget.dart';

class AudioPage extends StatelessWidget {
  const AudioPage({
    super.key,
    required this.isZh,
    required this.eqEnabled,
    required this.eqLowDb,
    required this.eqMidDb,
    required this.eqHighDb,
    required this.onSetEq,
    required this.onApplyEqPreset,
    required this.loudnessNormalizationEnabled,
    required this.onSetLoudnessNormalization,
    required this.micService,
    required this.micEnabled,
    required this.serviceTargetHost,
    required this.reverseChannelPort,
    required this.onToggleMic,
    required this.showJitterGraph,
    required this.jitterHistoryUs,
    required this.jitterP50Us,
    required this.jitterP95Us,
    required this.jitterUnderrun,
    required this.wsConnected,
    required this.playbackLabel,
    required this.audioLog,
    required this.onStartPlayback,
    required this.onStopPlayback,
    required this.playbackStopped,
    required this.currentModeLabel,
    required this.underrunCount,
  });

  final bool isZh;
  final bool eqEnabled;
  final int eqLowDb;
  final int eqMidDb;
  final int eqHighDb;
  final void Function({bool? enabled, int? lowDb, int? midDb, int? highDb})
      onSetEq;
  final void Function(String preset) onApplyEqPreset;
  final bool loudnessNormalizationEnabled;
  final ValueChanged<bool> onSetLoudnessNormalization;
  final MicCaptureService micService;
  final bool micEnabled;
  final String? serviceTargetHost;
  final int reverseChannelPort;
  final Future<void> Function() onToggleMic;
  final bool showJitterGraph;
  final List<int> jitterHistoryUs;
  final int jitterP50Us;
  final int jitterP95Us;
  final int jitterUnderrun;
  final bool wsConnected;
  final String playbackLabel;
  final String audioLog;
  final VoidCallback onStartPlayback;
  final VoidCallback onStopPlayback;
  final bool playbackStopped;
  final String currentModeLabel;
  final int underrunCount;

  String tr(String zh, String en) => isZh ? zh : en;

  @override
  Widget build(BuildContext context) {
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 10, 16, 20),
      children: [
        // Playback control card
        Card(
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(tr('播放', 'Playback'),
                    style: const TextStyle(
                        fontWeight: FontWeight.w700, fontSize: 16)),
                const SizedBox(height: 8),
                FilledButton(
                  onPressed: !wsConnected
                      ? null
                      : (playbackStopped ? onStartPlayback : onStopPlayback),
                  child: Text(
                    playbackStopped
                        ? tr('开始播放', 'Start Playback')
                        : tr('停止播放', 'Stop Playback'),
                  ),
                ),
                const SizedBox(height: 10),
                Text(
                  '${tr('当前模式', 'Current mode')}: $currentModeLabel',
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
                const SizedBox(height: 6),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    _metricTile(tr('状态', 'Status'), playbackLabel),
                    if (underrunCount > 0)
                      _metricTile(tr('欠载', 'Underrun'), '$underrunCount'),
                  ],
                ),
                const SizedBox(height: 4),
                AnimatedSize(
                  duration: const Duration(milliseconds: 200),
                  child: showJitterGraph
                      ? JitterGraphWidget(
                          jitterUs: jitterHistoryUs,
                          p50Us: jitterP50Us,
                          p95Us: jitterP95Us,
                          underrunCount: jitterUnderrun,
                        )
                      : const SizedBox.shrink(),
                ),
                const SizedBox(height: 8),
                MicStatusWidget(
                  service: micService,
                  host: serviceTargetHost,
                  reversePort: reverseChannelPort,
                  enabled: micEnabled,
                  onToggle: onToggleMic,
                ),
                if (audioLog.isNotEmpty) ...[
                  const SizedBox(height: 8),
                  Text('${tr('音频日志', 'Audio log')}: $audioLog'),
                ],
              ],
            ),
          ),
        ),
        const SizedBox(height: 10),
        // Equalizer card
        Card(
          child: Padding(
            padding: const EdgeInsets.all(8),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        tr('均衡器', 'Equalizer'),
                        style: const TextStyle(
                          fontWeight: FontWeight.w700,
                          fontSize: 14,
                        ),
                      ),
                    ),
                    Switch(
                      value: eqEnabled,
                      onChanged: (value) => onSetEq(enabled: value),
                    ),
                  ],
                ),
                const SizedBox(height: 6),
                Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: [
                    _eqPresetButton(tr('平直', 'Flat'), 'flat'),
                    _eqPresetButton(tr('低音增强', 'Bass'), 'bass'),
                    _eqPresetButton(tr('人声清晰', 'Vocal'), 'vocal'),
                    _eqPresetButton(tr('高频亮丽', 'Bright'), 'bright'),
                  ],
                ),
                const SizedBox(height: 6),
                SwitchListTile(
                  contentPadding: EdgeInsets.zero,
                  dense: true,
                  title: Text(
                    tr('响度归一化', 'Loudness normalization'),
                    style: const TextStyle(fontSize: 13),
                  ),
                  subtitle: Text(
                    tr(
                      '均衡/高质量模式生效，低延迟模式自动旁路',
                      'Active in balanced/high_quality; bypassed in low_latency',
                    ),
                    style: const TextStyle(fontSize: 11),
                  ),
                  value: loudnessNormalizationEnabled,
                  onChanged: onSetLoudnessNormalization,
                ),
                const SizedBox(height: 6),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceEvenly,
                  children: [
                    _eqSlider(
                      label: tr('低频\n60Hz', 'Low\n60Hz'),
                      value: eqLowDb,
                      onChanged: (value) => onSetEq(lowDb: value),
                    ),
                    _eqSlider(
                      label: tr('中频\n1kHz', 'Mid\n1kHz'),
                      value: eqMidDb,
                      onChanged: (value) => onSetEq(midDb: value),
                    ),
                    _eqSlider(
                      label: tr('高频\n10kHz', 'High\n10kHz'),
                      value: eqHighDb,
                      onChanged: (value) => onSetEq(highDb: value),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }

  Widget _eqSlider({
    required String label,
    required int value,
    required ValueChanged<int> onChanged,
  }) {
    return SizedBox(
      width: 86,
      height: 190,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(label, textAlign: TextAlign.center),
          const SizedBox(height: 4),
          Text(
            '${value >= 0 ? '+' : ''}$value dB',
            style: const TextStyle(fontWeight: FontWeight.w700),
          ),
          Expanded(
            child: RotatedBox(
              quarterTurns: -1,
              child: Slider(
                min: -10,
                max: 10,
                divisions: 20,
                value: value.toDouble(),
                onChanged: (next) => onChanged(next.round()),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _eqPresetButton(String label, String preset) {
    return OutlinedButton(
      onPressed: () => onApplyEqPreset(preset),
      child: Text(label),
    );
  }

  Widget _metricTile(String label, String value) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      decoration: BoxDecoration(
        border: Border.all(color: Colors.black12),
        borderRadius: BorderRadius.circular(12),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(label,
              style: const TextStyle(fontSize: 11, color: Colors.black54)),
          const SizedBox(height: 2),
          Text(value, style: const TextStyle(fontWeight: FontWeight.w700)),
        ],
      ),
    );
  }
}
