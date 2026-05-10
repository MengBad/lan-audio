bool isRemoteVersionNewer(String latest, String current) {
  final latestParts = _parseVersion(latest);
  final currentParts = _parseVersion(current);
  if (latestParts.isEmpty || currentParts.isEmpty) {
    return false;
  }
  final maxLength = latestParts.length > currentParts.length
      ? latestParts.length
      : currentParts.length;
  for (var i = 0; i < maxLength; i++) {
    final latestPart = i < latestParts.length ? latestParts[i] : 0;
    final currentPart = i < currentParts.length ? currentParts[i] : 0;
    if (latestPart > currentPart) {
      return true;
    }
    if (latestPart < currentPart) {
      return false;
    }
  }
  return false;
}

List<int> _parseVersion(String raw) {
  final normalized = raw.trim().replaceFirst(RegExp(r'^[vV]'), '');
  return normalized
      .split('.')
      .map((part) => int.tryParse(part))
      .whereType<int>()
      .toList(growable: false);
}
