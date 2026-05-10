import 'package:flutter_test/flutter_test.dart';
import 'package:lan_audio_android_mvp/connect_history.dart';

void main() {
  test('history upsert increments and keeps latest connection first', () {
    final first = DateTime.utc(2026, 5, 10, 10);
    final second = first.add(const Duration(minutes: 1));

    final entries = ConnectHistoryStore.upsert(
      const <ConnectHistoryEntry>[],
      ip: '192.168.1.10',
      port: 39991,
      hostname: 'LAN Audio @ Desk',
      connectedAt: first,
      latencyMs: 12,
    );
    final updated = ConnectHistoryStore.upsert(
      entries,
      ip: '192.168.1.10',
      port: 39991,
      hostname: 'LAN Audio @ Desk',
      connectedAt: second,
      latencyMs: 9,
    );

    expect(updated, hasLength(1));
    expect(updated.single.connectCount, 2);
    expect(updated.single.lastLatencyMs, 9);
    expect(updated.single.lastConnected, second);
  });

  test('favorites sort before recent non-favorites and cap at ten', () {
    final base = DateTime.utc(2026, 5, 10);
    final entries = List.generate(12, (index) {
      return ConnectHistoryEntry(
        ip: '192.168.1.$index',
        port: 39991,
        hostname: 'Device $index',
        lastConnected: base.add(Duration(minutes: index)),
        connectCount: index + 1,
        isFavorite: index == 2,
        lastLatencyMs: index,
      );
    });

    final sorted = ConnectHistoryStore.sortAndTrim(entries);
    expect(sorted, hasLength(10));
    expect(sorted.first.ip, '192.168.1.2');
    expect(sorted.any((entry) => entry.ip == '192.168.1.0'), false);
  });

  test('encode and decode preserves fields', () {
    final raw = ConnectHistoryStore.encode([
      ConnectHistoryEntry(
        ip: '10.0.0.8',
        port: 39991,
        hostname: 'Studio',
        lastConnected: DateTime.utc(2026, 5, 10),
        connectCount: 4,
        isFavorite: true,
        lastLatencyMs: 6,
      ),
    ]);

    final decoded = ConnectHistoryStore.decode(raw);
    expect(decoded.single.hostname, 'Studio');
    expect(decoded.single.isFavorite, true);
    expect(decoded.single.connectCount, 4);
  });
}
