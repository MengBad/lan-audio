use lan_audio_domain::{
    mode_contract, AudioMode, ConnectionState, FailureCode, ReleaseGate, ServiceSnapshot,
};

fn main() {
    let bundle = serde_json::json!({
        "mode_contracts": [
            mode_contract(AudioMode::LowLatency),
            mode_contract(AudioMode::Balanced),
            mode_contract(AudioMode::HighQuality),
        ],
        "connection_states": [
            ConnectionState::Disconnected,
            ConnectionState::Handshaking,
            ConnectionState::Negotiated,
            ConnectionState::Streaming,
            ConnectionState::Reconfiguring,
            ConnectionState::Recovering,
            ConnectionState::Closed,
        ],
        "failure_codes": [
            FailureCode::BuildFmt,
            FailureCode::BuildCheck,
            FailureCode::BuildTest,
            FailureCode::FlutterAnalyze,
            FailureCode::FlutterTest,
            FailureCode::GradleBuild,
            FailureCode::DeviceNotFound,
            FailureCode::AdbUnauthorized,
            FailureCode::UsbTetheringUnavailable,
            FailureCode::HandshakeTimeout,
            FailureCode::NegotiationMismatch,
            FailureCode::CodecInitFail,
            FailureCode::JitterGrowth,
            FailureCode::LatePacketStorm,
            FailureCode::AudioSinkUnderrun,
            FailureCode::BackgroundKilled,
            FailureCode::AudioFocusLost,
            FailureCode::ReconnectLoop,
            FailureCode::MetricsSchemaDrift,
            FailureCode::ReleaseGateBlocked,
        ],
        "service_snapshot_template": ServiceSnapshot::empty(),
        "release_gate_template": ReleaseGate::blocked(),
    });

    println!("{}", serde_json::to_string_pretty(&bundle).unwrap());
}
