import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android/audio/background_playback_service.dart';

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
      'eq_enabled': true,
      'reconnect_attempts': 3,
      'reconnect_delay_ms': 4000,
      'eq_settings': <String, dynamic>{
        'enabled': true,
        'low_db': 6,
        'mid_db': 0,
        'high_db': 3,
      },
      'loudness_normalization_enabled': true,
      'metrics': <String, dynamic>{
        'buffered_ms': 42,
        'underrun': 0,
        'late_packets': 1,
        'dropped_packets': 2,
        'rtt_ms': 3,
        'reconnect_count': 4,
        'decode_errors': 5,
        'sink_write_gap_ms_p95': 6,
        'loudness_gain_db': 2.3,
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
    expect(snapshot.eqEnabled, true);
    expect(snapshot.reconnectAttempts, 3);
    expect(snapshot.reconnectDelayMs, 4000);
    expect(snapshot.eqSettings['low_db'], 6);
    expect(snapshot.eqSettings['high_db'], 3);
    expect(snapshot.loudnessNormalizationEnabled, true);
    expect(snapshot.metrics['buffered_ms'], 42);
    expect(snapshot.metrics['loudness_gain_db'], 2.3);
    expect(snapshot.metrics['sink_write_gap_ms_p95'], 6);
  });

  test('PlaybackServiceSnapshot round-trips persisted EQ state fields', () {
    final snapshot = PlaybackServiceSnapshot.fromMap(const <String, dynamic>{
      'eq_enabled': true,
      'eq_settings': <String, dynamic>{
        'enabled': true,
        'low_db': -10,
        'mid_db': 4,
        'high_db': 10,
      },
    });

    final mapped = snapshot.toMap();
    expect(mapped['eq_enabled'], true);
    expect(mapped['eq_settings'], isA<Map<String, dynamic>>());
    expect((mapped['eq_settings'] as Map<String, dynamic>)['mid_db'], 4);
  });

  test('PlaybackServiceSnapshot exposes reconnect attempt fields', () {
    final snapshot = PlaybackServiceSnapshot.fromMap(const <String, dynamic>{
      'state': 'recovering',
      'reconnect_attempts': 5,
      'reconnect_delay_ms': 16000,
    });

    expect(snapshot.state, 'recovering');
    expect(snapshot.reconnectAttempts, 5);
    expect(snapshot.reconnectDelayMs, 16000);
    expect(snapshot.toMap()['reconnect_attempts'], 5);
  });
}
