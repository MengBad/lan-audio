use lan_audio_protocol::AudioMode;
use lan_audio_server::transport::run_opus_encoder_stress;

#[test]
fn opus_five_minute_stress_keeps_p99_and_drop_rate_in_bounds() {
    let stats = run_opus_encoder_stress(30_000, AudioMode::Balanced)
        .expect("opus stress helper should encode 5 minutes of aligned opus frames");

    eprintln!(
        "opus_stress_5m: encoded_packets={} p99_us={} drop_rate={:.6}",
        stats.encoded_packets, stats.p99_encode_us, stats.channel_full_drop_rate
    );

    assert_eq!(stats.encoded_packets, 15_000);
    assert!(
        stats.p99_encode_us <= 5_000,
        "opus encode p99 too high: {}us",
        stats.p99_encode_us
    );
    assert!(
        stats.channel_full_drop_rate <= 0.001,
        "channel full drop rate too high: {:.6}",
        stats.channel_full_drop_rate
    );
}
