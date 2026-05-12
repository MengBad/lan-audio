class MetricDisplaySampler {
  const MetricDisplaySampler({this.interval = const Duration(seconds: 1)});

  final Duration interval;

  bool canPublish({
    required DateTime now,
    required DateTime lastPublishedAt,
    required String runtimeState,
  }) {
    if (!_isActive(runtimeState)) return false;
    if (lastPublishedAt.millisecondsSinceEpoch == 0) return true;
    return now.difference(lastPublishedAt) >= interval;
  }

  bool _isActive(String runtimeState) {
    switch (runtimeState.toLowerCase()) {
      case 'streaming':
      case 'reconfiguring':
      case 'recovering':
        return true;
      default:
        return false;
    }
  }
}
