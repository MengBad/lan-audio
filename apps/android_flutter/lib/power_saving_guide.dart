class PowerSavingGuideStep {
  const PowerSavingGuideStep({
    required this.brand,
    required this.zh,
    required this.en,
  });

  final String brand;
  final String zh;
  final String en;
}

const List<PowerSavingGuideStep> powerSavingGuideSteps = [
  PowerSavingGuideStep(
    brand: 'xiaomi',
    zh: '小米设备：设置 -> 应用 -> LAN Audio -> 省电策略 -> 无限制',
    en: 'Xiaomi: Settings -> Apps -> LAN Audio -> Battery saver -> No restrictions',
  ),
  PowerSavingGuideStep(
    brand: 'huawei',
    zh: '华为设备：设置 -> 应用 -> 启动管理 -> LAN Audio -> 手动管理',
    en: 'Huawei: Settings -> Apps -> Launch management -> LAN Audio -> Manage manually',
  ),
  PowerSavingGuideStep(
    brand: 'generic',
    zh: '通用方案：设置 -> 电池 -> 受保护应用 -> 添加 LAN Audio',
    en: 'General: Settings -> Battery -> Protected apps -> Add LAN Audio',
  ),
];

List<PowerSavingGuideStep> orderedPowerSavingGuideSteps(String manufacturer) {
  final normalized = manufacturer.trim().toLowerCase();
  final matched = powerSavingGuideSteps.where((step) {
    return step.brand != 'generic' && normalized.contains(step.brand);
  }).toList();
  final generic =
      powerSavingGuideSteps.where((step) => step.brand == 'generic');
  final others = powerSavingGuideSteps.where((step) {
    return step.brand != 'generic' && !matched.contains(step);
  });
  return [...matched, ...generic, ...others];
}
