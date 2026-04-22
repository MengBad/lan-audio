import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android_mvp/audio/background_playback_service.dart';

void main() {
  test('PlaybackServiceSnapshot parses stable service snapshot contract', () {
    final snapshot = PlaybackServiceSnapshot.fromMap(const <String, dynamic>{
      'transport': 'usb',
      'mode': 'low_latency',
      'data_plane': 'v2_header',
      'active_data_plane': 'usb_direct',
      'rollback_available': true,
      'codec': 'opus',
      'effective_codec': 'opus',
      'state': 'streaming',
      'rollback_state': 'main_path_active',
      'metrics': <String, dynamic>{
        'buffered_ms': 42,
        'underrun': 0,
        'late_packets': 1,
        'dropped_packets': 2,
        'rtt_ms': 3,
        'reconnect_count': 4,
        'decode_errors': 5,
        'sink_write_gap_ms_p95': 6,
      },
    });

    expect(snapshot.transport, 'usb');
    expect(snapshot.mode, 'low_latency');
    expect(snapshot.dataPlane, 'v2_header');
    expect(snapshot.activeDataPlane, 'usb_direct');
    expect(snapshot.rollbackAvailable, true);
    expect(snapshot.codec, 'opus');
    expect(snapshot.effectiveCodec, 'opus');
    expect(snapshot.state, 'streaming');
    expect(snapshot.rollbackState, 'main_path_active');
    expect(snapshot.metrics['buffered_ms'], 42);
    expect(snapshot.metrics['sink_write_gap_ms_p95'], 6);
  });
}
