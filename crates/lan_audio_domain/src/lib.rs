use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AudioMode {
    LowLatency,
    Balanced,
    HighQuality,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AudioCodecPreference {
    Pcm16,
    #[serde(alias = "opus_experimental")]
    Opus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TransportType {
    Wifi,
    Usb,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DataPlanePath {
    LegacyLas1,
    V2Header,
    UsbDirect,
}

impl DataPlanePath {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LegacyLas1 => "legacy_las1",
            Self::V2Header => "v2_header",
            Self::UsbDirect => "usb_direct",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LatePolicy {
    AggressiveDrop,
    BoundedDrop,
    ContinuityFirst,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryPolicy {
    FastResync,
    SmoothResync,
    ContinuityFirst,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum OutputBackend {
    FastPath,
    AudioTrack,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct BufferTargetMs {
    pub min: u16,
    pub max: u16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaybackTuning {
    pub start_buffer_ms: u16,
    pub max_buffer_ms: u16,
    pub batch_frames: u8,
    pub drop_threshold_ms: u16,
    pub frame_duration_ms: u16,
    pub reset_buffer_on_switch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModeContract {
    pub mode: AudioMode,
    pub preferred_transport: Vec<TransportType>,
    pub preferred_codec: AudioCodecPreference,
    pub target_buffer_ms: BufferTargetMs,
    pub late_policy: LatePolicy,
    pub recovery_policy: RecoveryPolicy,
    pub output_backend_priority: Vec<OutputBackend>,
    pub promise: String,
    pub tuning: PlaybackTuning,
}

pub fn mode_contract(mode: AudioMode) -> ModeContract {
    match mode {
        AudioMode::LowLatency => ModeContract {
            mode,
            preferred_transport: vec![TransportType::Usb, TransportType::Wifi],
            preferred_codec: AudioCodecPreference::Opus,
            target_buffer_ms: BufferTargetMs { min: 30, max: 50 },
            late_policy: LatePolicy::AggressiveDrop,
            recovery_policy: RecoveryPolicy::FastResync,
            output_backend_priority: vec![OutputBackend::FastPath, OutputBackend::AudioTrack],
            promise: "latency first".to_string(),
            tuning: PlaybackTuning {
                start_buffer_ms: 40,
                max_buffer_ms: 180,
                batch_frames: 1,
                drop_threshold_ms: 140,
                frame_duration_ms: 10,
                reset_buffer_on_switch: true,
            },
        },
        AudioMode::Balanced => ModeContract {
            mode,
            preferred_transport: vec![TransportType::Usb, TransportType::Wifi],
            preferred_codec: AudioCodecPreference::Opus,
            target_buffer_ms: BufferTargetMs { min: 60, max: 90 },
            late_policy: LatePolicy::BoundedDrop,
            recovery_policy: RecoveryPolicy::SmoothResync,
            output_backend_priority: vec![OutputBackend::AudioTrack, OutputBackend::FastPath],
            promise: "latency/stability balance".to_string(),
            tuning: PlaybackTuning {
                start_buffer_ms: 60,
                max_buffer_ms: 300,
                batch_frames: 2,
                drop_threshold_ms: 220,
                frame_duration_ms: 10,
                reset_buffer_on_switch: true,
            },
        },
        AudioMode::HighQuality => ModeContract {
            mode,
            preferred_transport: vec![TransportType::Usb, TransportType::Wifi],
            preferred_codec: AudioCodecPreference::Opus,
            target_buffer_ms: BufferTargetMs { min: 100, max: 150 },
            late_policy: LatePolicy::ContinuityFirst,
            recovery_policy: RecoveryPolicy::ContinuityFirst,
            output_backend_priority: vec![OutputBackend::AudioTrack, OutputBackend::FastPath],
            promise: "quality/stability first".to_string(),
            tuning: PlaybackTuning {
                start_buffer_ms: 120,
                max_buffer_ms: 500,
                batch_frames: 3,
                drop_threshold_ms: 420,
                frame_duration_ms: 10,
                reset_buffer_on_switch: false,
            },
        },
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Handshaking,
    Negotiated,
    Streaming,
    Reconfiguring,
    Recovering,
    Closed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RollbackState {
    MainPathActive,
    ReconfiguringToLegacyLas1Pcm16,
    ForcedLegacyLas1Pcm16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FailureCode {
    BuildFmt,
    BuildCheck,
    BuildTest,
    FlutterAnalyze,
    FlutterTest,
    GradleBuild,
    DeviceNotFound,
    AdbUnauthorized,
    UsbTetheringUnavailable,
    HandshakeTimeout,
    NegotiationMismatch,
    CodecInitFail,
    JitterGrowth,
    LatePacketStorm,
    AudioSinkUnderrun,
    BackgroundKilled,
    AudioFocusLost,
    ReconnectLoop,
    MetricsSchemaDrift,
    ReleaseGateBlocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceMetricsSnapshot {
    pub buffered_ms: u32,
    pub underrun: u64,
    pub late_packets: u64,
    pub dropped_packets: u64,
    pub rtt_ms: u32,
    pub reconnect_count: u64,
    pub decode_errors: u64,
    pub sink_write_gap_ms_p95: u32,
}

impl Default for ServiceMetricsSnapshot {
    fn default() -> Self {
        Self {
            buffered_ms: 0,
            underrun: 0,
            late_packets: 0,
            dropped_packets: 0,
            rtt_ms: 0,
            reconnect_count: 0,
            decode_errors: 0,
            sink_write_gap_ms_p95: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceSnapshot {
    pub transport: TransportType,
    pub mode: AudioMode,
    pub data_plane: DataPlanePath,
    pub active_data_plane: DataPlanePath,
    pub rollback_available: bool,
    pub codec: AudioCodecPreference,
    pub effective_codec: AudioCodecPreference,
    pub state: ConnectionState,
    pub rollback_state: RollbackState,
    pub metrics: ServiceMetricsSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<serde_json::Value>,
}

impl ServiceSnapshot {
    pub fn empty() -> Self {
        Self {
            transport: TransportType::Wifi,
            mode: AudioMode::Balanced,
            data_plane: DataPlanePath::LegacyLas1,
            active_data_plane: DataPlanePath::LegacyLas1,
            rollback_available: true,
            codec: AudioCodecPreference::Pcm16,
            effective_codec: AudioCodecPreference::Pcm16,
            state: ConnectionState::Disconnected,
            rollback_state: RollbackState::MainPathActive,
            metrics: ServiceMetricsSnapshot::default(),
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseDecision {
    AllowRelease,
    ContinueFixing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EffectiveConfigSummary {
    pub transport: TransportType,
    pub mode: AudioMode,
    pub data_plane: DataPlanePath,
    pub codec: AudioCodecPreference,
    pub effective_codec: AudioCodecPreference,
    pub rollback_state: RollbackState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseGate {
    pub contract_version: u8,
    pub release_decision: ReleaseDecision,
    pub current_main_path: EffectiveConfigSummary,
    pub rollback_path: EffectiveConfigSummary,
    pub validate_local_passed: bool,
    pub rewrite_validate_passed: bool,
    pub device_acceptance_passed: bool,
    pub acceptance_json_present: bool,
    pub rollback_verified: bool,
    pub android_release_apk_present: bool,
    pub windows_exe_present: bool,
    pub known_blockers: u32,
    pub critical_bugs: u32,
    pub blocking_failure_codes: Vec<FailureCode>,
}

impl ReleaseGate {
    pub fn blocked() -> Self {
        Self {
            contract_version: 1,
            release_decision: ReleaseDecision::ContinueFixing,
            current_main_path: EffectiveConfigSummary {
                transport: TransportType::Wifi,
                mode: AudioMode::Balanced,
                data_plane: DataPlanePath::V2Header,
                codec: AudioCodecPreference::Opus,
                effective_codec: AudioCodecPreference::Opus,
                rollback_state: RollbackState::MainPathActive,
            },
            rollback_path: EffectiveConfigSummary {
                transport: TransportType::Wifi,
                mode: AudioMode::Balanced,
                data_plane: DataPlanePath::LegacyLas1,
                codec: AudioCodecPreference::Pcm16,
                effective_codec: AudioCodecPreference::Pcm16,
                rollback_state: RollbackState::ForcedLegacyLas1Pcm16,
            },
            validate_local_passed: false,
            rewrite_validate_passed: false,
            device_acceptance_passed: false,
            acceptance_json_present: false,
            rollback_verified: false,
            android_release_apk_present: false,
            windows_exe_present: false,
            known_blockers: 1,
            critical_bugs: 0,
            blocking_failure_codes: vec![FailureCode::ReleaseGateBlocked],
        }
    }

    pub fn allows_release(&self) -> bool {
        self.release_decision == ReleaseDecision::AllowRelease
            && self.validate_local_passed
            && self.rewrite_validate_passed
            && self.device_acceptance_passed
            && self.acceptance_json_present
            && self.rollback_verified
            && self.android_release_apk_present
            && self.windows_exe_present
            && self.known_blockers == 0
            && self.critical_bugs == 0
            && self.blocking_failure_codes.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionStateMachine {
    state: ConnectionState,
    rollback_state: RollbackState,
    failure_code: Option<FailureCode>,
}

impl Default for ConnectionStateMachine {
    fn default() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            rollback_state: RollbackState::MainPathActive,
            failure_code: None,
        }
    }
}

impl ConnectionStateMachine {
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    pub fn rollback_state(&self) -> RollbackState {
        self.rollback_state
    }

    pub fn failure_code(&self) -> Option<FailureCode> {
        self.failure_code
    }

    pub fn transition(&mut self, next: ConnectionState) -> Result<(), TransitionError> {
        if !is_allowed_transition(self.state, next) {
            return Err(TransitionError::invalid(self.state, next));
        }
        self.state = next;
        self.failure_code = None;
        if self.rollback_state == RollbackState::ReconfiguringToLegacyLas1Pcm16
            && matches!(
                next,
                ConnectionState::Negotiated | ConnectionState::Streaming
            )
        {
            self.rollback_state = RollbackState::ForcedLegacyLas1Pcm16;
        }
        Ok(())
    }

    pub fn fail(
        &mut self,
        next: ConnectionState,
        failure_code: FailureCode,
    ) -> Result<(), TransitionError> {
        if !matches!(next, ConnectionState::Recovering | ConnectionState::Closed) {
            return Err(TransitionError::failure_target(self.state, next));
        }
        if !is_allowed_transition(self.state, next) {
            return Err(TransitionError::invalid(self.state, next));
        }
        self.state = next;
        self.failure_code = Some(failure_code);
        Ok(())
    }

    pub fn force_rollback(&mut self) -> Result<(), TransitionError> {
        if matches!(self.state, ConnectionState::Closed) {
            return Err(TransitionError::invalid(
                self.state,
                ConnectionState::Handshaking,
            ));
        }
        self.rollback_state = RollbackState::ReconfiguringToLegacyLas1Pcm16;
        self.failure_code = None;
        self.state = ConnectionState::Handshaking;
        Ok(())
    }
}

fn is_allowed_transition(from: ConnectionState, to: ConnectionState) -> bool {
    match from {
        ConnectionState::Disconnected => {
            matches!(to, ConnectionState::Handshaking | ConnectionState::Closed)
        }
        ConnectionState::Handshaking => {
            matches!(
                to,
                ConnectionState::Negotiated | ConnectionState::Recovering | ConnectionState::Closed
            )
        }
        ConnectionState::Negotiated => {
            matches!(
                to,
                ConnectionState::Streaming
                    | ConnectionState::Reconfiguring
                    | ConnectionState::Recovering
                    | ConnectionState::Closed
            )
        }
        ConnectionState::Streaming => {
            matches!(
                to,
                ConnectionState::Reconfiguring
                    | ConnectionState::Recovering
                    | ConnectionState::Closed
            )
        }
        ConnectionState::Reconfiguring => {
            matches!(
                to,
                ConnectionState::Negotiated
                    | ConnectionState::Streaming
                    | ConnectionState::Recovering
                    | ConnectionState::Closed
            )
        }
        ConnectionState::Recovering => {
            matches!(
                to,
                ConnectionState::Handshaking
                    | ConnectionState::Negotiated
                    | ConnectionState::Streaming
                    | ConnectionState::Closed
            )
        }
        ConnectionState::Closed => false,
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum TransitionError {
    #[error("invalid transition {from:?} -> {to:?}")]
    Invalid {
        from: ConnectionState,
        to: ConnectionState,
    },
    #[error("abnormal transition target must be recovering or closed: {from:?} -> {to:?}")]
    InvalidFailureTarget {
        from: ConnectionState,
        to: ConnectionState,
    },
}

impl TransitionError {
    fn invalid(from: ConnectionState, to: ConnectionState) -> Self {
        Self::Invalid { from, to }
    }

    fn failure_target(from: ConnectionState, to: ConnectionState) -> Self {
        Self::InvalidFailureTarget { from, to }
    }
}

impl Default for AudioMode {
    fn default() -> Self {
        Self::Balanced
    }
}

impl Default for AudioCodecPreference {
    fn default() -> Self {
        Self::Pcm16
    }
}

impl Default for TransportType {
    fn default() -> Self {
        Self::Wifi
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn low_latency_mode_contract_prefers_usb_and_opus() {
        let contract = mode_contract(AudioMode::LowLatency);
        assert_eq!(
            contract.preferred_transport,
            vec![TransportType::Usb, TransportType::Wifi]
        );
        assert_eq!(contract.preferred_codec, AudioCodecPreference::Opus);
        assert_eq!(
            contract.target_buffer_ms,
            BufferTargetMs { min: 30, max: 50 }
        );
        assert_eq!(contract.promise, "latency first");
    }

    #[test]
    fn balanced_and_high_quality_contracts_keep_continuity_biases() {
        let balanced = mode_contract(AudioMode::Balanced);
        let high = mode_contract(AudioMode::HighQuality);
        assert_eq!(balanced.late_policy, LatePolicy::BoundedDrop);
        assert_eq!(balanced.recovery_policy, RecoveryPolicy::SmoothResync);
        assert_eq!(high.late_policy, LatePolicy::ContinuityFirst);
        assert_eq!(high.recovery_policy, RecoveryPolicy::ContinuityFirst);
        assert_eq!(high.target_buffer_ms, BufferTargetMs { min: 100, max: 150 });
    }

    #[test]
    fn state_machine_accepts_happy_path() {
        let mut machine = ConnectionStateMachine::default();
        machine.transition(ConnectionState::Handshaking).unwrap();
        machine.transition(ConnectionState::Negotiated).unwrap();
        machine.transition(ConnectionState::Streaming).unwrap();
        assert_eq!(machine.state(), ConnectionState::Streaming);
        assert_eq!(machine.failure_code(), None);
    }

    #[test]
    fn invalid_transition_is_rejected() {
        let mut machine = ConnectionStateMachine::default();
        let err = machine.transition(ConnectionState::Streaming).unwrap_err();
        assert_eq!(
            err,
            TransitionError::Invalid {
                from: ConnectionState::Disconnected,
                to: ConnectionState::Streaming,
            }
        );
    }

    #[test]
    fn abnormal_exit_requires_explicit_failure_path() {
        let mut machine = ConnectionStateMachine::default();
        machine.transition(ConnectionState::Handshaking).unwrap();
        machine.transition(ConnectionState::Negotiated).unwrap();
        machine.transition(ConnectionState::Streaming).unwrap();
        machine
            .fail(ConnectionState::Recovering, FailureCode::HandshakeTimeout)
            .unwrap();
        assert_eq!(machine.state(), ConnectionState::Recovering);
        assert_eq!(machine.failure_code(), Some(FailureCode::HandshakeTimeout));
    }

    #[test]
    fn force_rollback_reenters_handshaking_and_marks_state() {
        let mut machine = ConnectionStateMachine::default();
        machine.transition(ConnectionState::Handshaking).unwrap();
        machine.transition(ConnectionState::Negotiated).unwrap();
        machine.transition(ConnectionState::Streaming).unwrap();
        machine.force_rollback().unwrap();
        assert_eq!(machine.state(), ConnectionState::Handshaking);
        assert_eq!(
            machine.rollback_state(),
            RollbackState::ReconfiguringToLegacyLas1Pcm16
        );
        machine.transition(ConnectionState::Negotiated).unwrap();
        assert_eq!(
            machine.rollback_state(),
            RollbackState::ForcedLegacyLas1Pcm16
        );
    }

    #[test]
    fn service_snapshot_serializes_exact_public_schema() {
        let snapshot = ServiceSnapshot::empty();
        let value = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(
            value,
            json!({
                "transport": "wifi",
                "mode": "balanced",
                "data_plane": "legacy_las1",
                "active_data_plane": "legacy_las1",
                "rollback_available": true,
                "codec": "pcm16",
                "effective_codec": "pcm16",
                "state": "disconnected",
                "rollback_state": "main_path_active",
                "metrics": {
                    "buffered_ms": 0,
                    "underrun": 0,
                    "late_packets": 0,
                    "dropped_packets": 0,
                    "rtt_ms": 0,
                    "reconnect_count": 0,
                    "decode_errors": 0,
                    "sink_write_gap_ms_p95": 0
                }
            })
        );
    }

    #[test]
    fn blocked_release_gate_stays_closed() {
        let gate = ReleaseGate::blocked();
        assert_eq!(gate.release_decision, ReleaseDecision::ContinueFixing);
        assert_eq!(
            gate.blocking_failure_codes,
            vec![FailureCode::ReleaseGateBlocked]
        );
        assert!(!gate.allows_release());
    }
}
