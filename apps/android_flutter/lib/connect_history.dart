import 'dart:convert';

class ConnectHistoryEntry {
  const ConnectHistoryEntry({
    required this.ip,
    required this.port,
    required this.hostname,
    required this.lastConnected,
    required this.connectCount,
    required this.isFavorite,
    required this.lastLatencyMs,
  });

  final String ip;
  final int port;
  final String hostname;
  final DateTime lastConnected;
  final int connectCount;
  final bool isFavorite;
  final int lastLatencyMs;

  ConnectHistoryEntry copyWith({
    String? ip,
    int? port,
    String? hostname,
    DateTime? lastConnected,
    int? connectCount,
    bool? isFavorite,
    int? lastLatencyMs,
  }) {
    return ConnectHistoryEntry(
      ip: ip ?? this.ip,
      port: port ?? this.port,
      hostname: hostname ?? this.hostname,
      lastConnected: lastConnected ?? this.lastConnected,
      connectCount: connectCount ?? this.connectCount,
      isFavorite: isFavorite ?? this.isFavorite,
      lastLatencyMs: lastLatencyMs ?? this.lastLatencyMs,
    );
  }

  Map<String, dynamic> toJson() {
    return <String, dynamic>{
      'ip': ip,
      'port': port,
      'hostname': hostname,
      'last_connected': lastConnected.toIso8601String(),
      'connect_count': connectCount,
      'is_favorite': isFavorite,
      'last_latency_ms': lastLatencyMs,
    };
  }

  factory ConnectHistoryEntry.fromJson(Map<dynamic, dynamic> json) {
    return ConnectHistoryEntry(
      ip: '${json['ip'] ?? ''}',
      port: (json['port'] as num?)?.toInt() ?? 39991,
      hostname: '${json['hostname'] ?? 'Manual'}',
      lastConnected: DateTime.tryParse('${json['last_connected'] ?? ''}') ??
          DateTime.fromMillisecondsSinceEpoch(0),
      connectCount: (json['connect_count'] as num?)?.toInt() ?? 0,
      isFavorite: json['is_favorite'] == true,
      lastLatencyMs: (json['last_latency_ms'] as num?)?.toInt() ?? 0,
    );
  }
}

class ConnectHistoryStore {
  static const maxEntries = 10;

  static List<ConnectHistoryEntry> decode(String raw) {
    if (raw.trim().isEmpty) {
      return const <ConnectHistoryEntry>[];
    }
    final decoded = jsonDecode(raw);
    if (decoded is! List) {
      return const <ConnectHistoryEntry>[];
    }
    return sortAndTrim(decoded
        .whereType<Map>()
        .map(ConnectHistoryEntry.fromJson)
        .where((entry) => entry.ip.trim().isNotEmpty)
        .toList());
  }

  static String encode(List<ConnectHistoryEntry> entries) {
    return jsonEncode(sortAndTrim(entries).map((e) => e.toJson()).toList());
  }

  static List<ConnectHistoryEntry> upsert(
    List<ConnectHistoryEntry> entries, {
    required String ip,
    required int port,
    required String hostname,
    required DateTime connectedAt,
    required int latencyMs,
  }) {
    final normalizedHost = hostname.trim().isEmpty ? 'Manual' : hostname.trim();
    final next = <ConnectHistoryEntry>[];
    var inserted = false;
    for (final entry in entries) {
      if (entry.ip == ip && entry.port == port) {
        next.add(entry.copyWith(
          hostname: normalizedHost,
          lastConnected: connectedAt,
          connectCount: entry.connectCount + 1,
          lastLatencyMs: latencyMs,
        ));
        inserted = true;
      } else {
        next.add(entry);
      }
    }
    if (!inserted) {
      next.add(ConnectHistoryEntry(
        ip: ip,
        port: port,
        hostname: normalizedHost,
        lastConnected: connectedAt,
        connectCount: 1,
        isFavorite: false,
        lastLatencyMs: latencyMs,
      ));
    }
    return sortAndTrim(next);
  }

  static List<ConnectHistoryEntry> sortAndTrim(
    List<ConnectHistoryEntry> entries,
  ) {
    final sorted = [...entries]..sort((a, b) {
        if (a.isFavorite != b.isFavorite) {
          return a.isFavorite ? -1 : 1;
        }
        return b.lastConnected.compareTo(a.lastConnected);
      });
    return sorted.take(maxEntries).toList();
  }
}
