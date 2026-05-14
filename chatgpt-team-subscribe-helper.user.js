// ==UserScript==
// @name         ChatGPT Team 订阅助手 (终极稳定版)
// @namespace    http://tampermonkey.net/
// @version      7.1
// @description  在 ChatGPT 页面生成 Team 订阅支付链接，支持多国家/货币/优惠码
// @match        https://chatgpt.com/*
// @match        https://chat.openai.com/*
// @grant        GM_addStyle
// @grant        GM_openInTab
// @run-at       document-idle
// ==/UserScript==

(function() {
  'use strict';

  const COUNTRIES = [
    { code: "US", name: "美国", flag: "🇺🇸", currency: "USD", currencyName: "美元" },
    { code: "CA", name: "加拿大", flag: "🇨🇦", currency: "CAD", currencyName: "加元" },
    { code: "MX", name: "墨西哥", flag: "🇲🇽", currency: "MXN", currencyName: "比索" },
    { code: "BR", name: "巴西", flag: "🇧🇷", currency: "BRL", currencyName: "雷亚尔" },
    { code: "AR", name: "阿根廷", flag: "🇦🇷", currency: "ARS", currencyName: "比索" },
    { code: "CL", name: "智利", flag: "🇨🇱", currency: "CLP", currencyName: "比索" },
    { code: "CO", name: "哥伦比亚", flag: "🇨🇴", currency: "COP", currencyName: "比索" },
    { code: "PE", name: "秘鲁", flag: "🇵🇪", currency: "PEN", currencyName: "索尔" },
    { code: "GB", name: "英国", flag: "🇬🇧", currency: "GBP", currencyName: "英镑" },
    { code: "DE", name: "德国", flag: "🇩🇪", currency: "EUR", currencyName: "欧元" },
    { code: "FR", name: "法国", flag: "🇫🇷", currency: "EUR", currencyName: "欧元" },
    { code: "IT", name: "意大利", flag: "🇮🇹", currency: "EUR", currencyName: "欧元" },
    { code: "ES", name: "西班牙", flag: "🇪🇸", currency: "EUR", currencyName: "欧元" },
    { code: "NL", name: "荷兰", flag: "🇳🇱", currency: "EUR", currencyName: "欧元" },
    { code: "CH", name: "瑞士", flag: "🇨🇭", currency: "CHF", currencyName: "瑞士法郎" },
    { code: "SE", name: "瑞典", flag: "🇸🇪", currency: "SEK", currencyName: "克朗" },
    { code: "NO", name: "挪威", flag: "🇳🇴", currency: "NOK", currencyName: "克朗" },
    { code: "DK", name: "丹麦", flag: "🇩🇰", currency: "DKK", currencyName: "克朗" },
    { code: "PL", name: "波兰", flag: "🇵🇱", currency: "PLN", currencyName: "兹罗提" },
    { code: "TR", name: "土耳其", flag: "🇹🇷", currency: "TRY", currencyName: "里拉" },
    { code: "JP", name: "日本", flag: "🇯🇵", currency: "JPY", currencyName: "日元" },
    { code: "KR", name: "韩国", flag: "🇰🇷", currency: "KRW", currencyName: "韩元" },
    { code: "SG", name: "新加坡", flag: "🇸🇬", currency: "SGD", currencyName: "新元" },
    { code: "MY", name: "马来西亚", flag: "🇲🇾", currency: "MYR", currencyName: "林吉特" },
    { code: "ID", name: "印度尼西亚", flag: "🇮🇩", currency: "IDR", currencyName: "印尼盾" },
    { code: "PH", name: "菲律宾", flag: "🇵🇭", currency: "PHP", currencyName: "比索" },
    { code: "TH", name: "泰国", flag: "🇹🇭", currency: "THB", currencyName: "泰铢" },
    { code: "VN", name: "越南", flag: "🇻🇳", currency: "VND", currencyName: "越南盾" },
    { code: "IN", name: "印度", flag: "🇮🇳", currency: "INR", currencyName: "卢比" },
    { code: "PK", name: "巴基斯坦", flag: "🇵🇰", currency: "PKR", currencyName: "卢比" },
    { code: "BD", name: "孟加拉国", flag: "🇧🇩", currency: "BDT", currencyName: "塔卡" },
    { code: "AE", name: "阿联酋", flag: "🇦🇪", currency: "AED", currencyName: "迪拉姆" },
    { code: "SA", name: "沙特阿拉伯", flag: "🇸🇦", currency: "SAR", currencyName: "里亚尔" },
    { code: "IL", name: "以色列", flag: "🇮🇱", currency: "ILS", currencyName: "新谢克尔" },
    { code: "AU", name: "澳大利亚", flag: "🇦🇺", currency: "AUD", currencyName: "澳元" },
    { code: "NZ", name: "新西兰", flag: "🇳🇿", currency: "NZD", currencyName: "纽元" },
    { code: "ZA", name: "南非", flag: "🇿🇦", currency: "ZAR", currencyName: "兰特" },
    { code: "NG", name: "尼日利亚", flag: "🇳🇬", currency: "NGN", currencyName: "奈拉" },
    { code: "EG", name: "埃及", flag: "🇪🇬", currency: "EGP", currencyName: "镑" }
  ];

  const SUPPORTED_CURRENCIES = [
    { code: "USD", name: "美元 (USD)" }, { code: "EUR", name: "欧元 (EUR)" },
    { code: "GBP", name: "英镑 (GBP)" }, { code: "CAD", name: "加元 (CAD)" },
    { code: "AUD", name: "澳元 (AUD)" }, { code: "JPY", name: "日元 (JPY)" },
    { code: "SGD", name: "新加坡元 (SGD)" }, { code: "INR", name: "印度卢比 (INR)" },
    { code: "BRL", name: "巴西雷亚尔 (BRL)" }, { code: "MXN", name: "墨西哥比索 (MXN)" },
    { code: "CHF", name: "瑞士法郎 (CHF)" }, { code: "SEK", name: "瑞典克朗 (SEK)" },
    { code: "NOK", name: "挪威克朗 (NOK)" }, { code: "DKK", name: "丹麦克朗 (DKK)" },
    { code: "PLN", name: "波兰兹罗提 (PLN)" }, { code: "NZD", name: "新西兰元 (NZD)" },
    { code: "MYR", name: "马来西亚林吉特 (MYR)" }, { code: "IDR", name: "印尼盾 (IDR)" },
    { code: "PHP", name: "菲律宾比索 (PHP)" }, { code: "THB", name: "泰铢 (THB)" },
    { code: "VND", name: "越南盾 (VND)" }, { code: "KRW", name: "韩元 (KRW)" },
    { code: "AED", name: "阿联酋迪拉姆 (AED)" }, { code: "SAR", name: "沙特里亚尔 (SAR)" },
    { code: "ILS", name: "以色列新谢克尔 (ILS)" }, { code: "ZAR", name: "南非兰特 (ZAR)" },
    { code: "TRY", name: "土耳其里拉 (TRY)" }, { code: "ARS", name: "阿根廷比索 (ARS)" },
    { code: "CLP", name: "智利比索 (CLP)" }, { code: "COP", name: "哥伦比亚比索 (COP)" },
    { code: "PEN", name: "秘鲁索尔 (PEN)" }, { code: "PKR", name: "巴基斯坦卢比 (PKR)" },
    { code: "BDT", name: "孟加拉塔卡 (BDT)" }, { code: "NGN", name: "尼日利亚奈拉 (NGN)" },
    { code: "EGP", name: "埃及镑 (EGP)" }
  ];

  function getCountryDefaultCurrency(code) {
    const country = COUNTRIES.find(c => c.code === code);
    return country ? country.currency : "USD";
  }

  function isCurrencyMatchCountry(countryCode, currencyCode) {
    return getCountryDefaultCurrency(countryCode) === currencyCode;
  }

  function initUI() {
    GM_addStyle(`
      #team-fab {
        position: fixed !important;
        bottom: 24px !important;
        right: 24px !important;
        width: 48px !important;
        height: 48px !important;
        background: linear-gradient(135deg, #667eea 0%, #764ba2 100%) !important;
        border-radius: 50% !important;
        box-shadow: 0 4px 20px rgba(102, 126, 234, 0.4) !important;
        cursor: pointer !important;
        z-index: 2147483647 !important;
        display: flex !important;
        align-items: center !important;
        justify-content: center !important;
        transition: transform 0.2s ease !important;
      }
      #team-fab:hover { transform: scale(1.1) !important; }
      #team-fab svg { fill: white; width: 24px; height: 24px; }
      #team-debug-panel {
        position: fixed; bottom: 90px; right: 24px;
        width: 380px; max-height: 80vh; background: #fff; border-radius: 20px;
        box-shadow: 0 10px 40px rgba(0,0,0,0.15);
        z-index: 2147483646; display: none; overflow-y: auto;
        border: 1px solid #eee;
      }
      #team-debug-panel.show { display: block; }
      .tm-panel-header {
        background: linear-gradient(135deg, #667eea, #764ba2);
        padding: 16px 20px; position: relative;
      }
      .tm-panel-header h3 { margin: 0; color: #fff; font-size: 16px; }
      .tm-panel-header p { margin: 4px 0 0; color: rgba(255,255,255,0.8); font-size: 12px; }
      .tm-close-panel {
        position: absolute; top: 14px; right: 20px;
        color: rgba(255,255,255,0.8); font-size: 22px; cursor: pointer;
        background: none; border: none; line-height: 1;
      }
      .tm-close-panel:hover { color: #fff; }
      .tm-panel-content { padding: 20px; }
      .tm-input-group { margin-bottom: 16px; }
      .tm-input-group label { display: block; font-size: 12px; color: #666; margin-bottom: 6px; font-weight: 500; }
      .tm-input-group input, .tm-input-group select {
        width: 100%; padding: 10px 12px; border: 1px solid #ddd;
        border-radius: 10px; font-size: 13px; box-sizing: border-box;
        outline: none; transition: border-color 0.2s;
      }
      .tm-input-group input:focus, .tm-input-group select:focus {
        border-color: #667eea;
      }
      .tm-row-2cols { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
      #tm-exec-btn {
        width: 100%; padding: 12px; border: none; border-radius: 10px;
        background: linear-gradient(135deg, #667eea, #764ba2);
        color: #fff; font-weight: 600; font-size: 14px; cursor: pointer;
        transition: opacity 0.2s;
      }
      #tm-exec-btn:hover { opacity: 0.9; }
      #tm-exec-btn:disabled { opacity: 0.6; cursor: not-allowed; }
      .tm-match-warning {
        margin-top: 12px; padding: 8px 12px; border-radius: 8px;
        font-size: 11px; border: 1px solid;
      }
      #tm-err-log {
        margin-top: 16px; padding: 10px; border-radius: 8px;
        font-size: 11px; display: none; word-break: break-all;
      }
      #tm-err-log a { color: #667eea; text-decoration: underline; }
    `);

    // 防止重复初始化
    if (document.getElementById('team-fab')) return;

    // 创建浮动按钮
    const fab = document.createElement('div');
    fab.id = 'team-fab';
    fab.title = 'Team 订阅助手';
    fab.innerHTML = `<svg viewBox="0 0 24 24"><path d="M12 8c-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4-1.79-4-4-4zm8.94 3A8.994 8.994 0 0 0 13 3.06V1h-2v2.06A8.994 8.994 0 0 0 3.06 11H1v2h2.06A8.994 8.994 0 0 0 11 20.94V23h2v-2.06A8.994 8.994 0 0 0 20.94 13H23v-2h-2.06zM12 19c-3.87 0-7-3.13-7-7s3.13-7 7-7 7 3.13 7 7-3.87 7-7 7z"/></svg>`;
    document.body.appendChild(fab);

    // 创建面板
    const panel = document.createElement('div');
    panel.id = 'team-debug-panel';
    panel.innerHTML = `
      <div class="tm-panel-header">
        <button class="tm-close-panel" id="tm-close-panel-btn">&times;</button>
        <h3>✨ Team 订阅助手</h3>
        <p>多国家 / 货币 / 优惠码</p>
      </div>
      <div class="tm-panel-content">
        <div class="tm-input-group">
          <label>🏢 团队名称</label>
          <input type="text" id="tm-ws-name" value="MyTeam" placeholder="输入团队名称">
        </div>
        <div class="tm-row-2cols">
          <div class="tm-input-group">
            <label>🌍 国家/地区</label>
            <select id="tm-country-select"></select>
          </div>
          <div class="tm-input-group">
            <label>💵 货币</label>
            <select id="tm-currency-select"></select>
          </div>
        </div>
        <div id="tm-currency-match-warning" class="tm-match-warning"></div>
        <div class="tm-input-group">
          <label>👥 席位数量</label>
          <input type="number" id="tm-seat-count" value="2" min="2" max="150">
        </div>
        <div class="tm-input-group">
          <label>🎁 优惠码 (选填)</label>
          <input type="text" id="tm-promo-id" placeholder="留空则不使用优惠码">
        </div>
        <button id="tm-exec-btn">🔗 生成支付链接</button>
        <div id="tm-err-log"></div>
      </div>`;
    document.body.appendChild(panel);

    // 获取 DOM 引用
    const countrySelect = document.getElementById('tm-country-select');
    const currencySelect = document.getElementById('tm-currency-select');
    const warningDiv = document.getElementById('tm-currency-match-warning');
    const logDiv = document.getElementById('tm-err-log');
    const execBtn = document.getElementById('tm-exec-btn');

    // 填充国家下拉
    COUNTRIES.forEach(c => {
      const o = document.createElement('option');
      o.value = c.code;
      o.textContent = `${c.flag} ${c.name}`;
      countrySelect.appendChild(o);
    });

    // 填充货币下拉
    SUPPORTED_CURRENCIES.forEach(c => {
      const o = document.createElement('option');
      o.value = c.code;
      o.textContent = c.name;
      currencySelect.appendChild(o);
    });

    countrySelect.value = 'US';
    currencySelect.value = 'USD';

    // 匹配提示
    function updateWarning() {
      const c = countrySelect.value;
      const cur = currencySelect.value;
      if (isCurrencyMatchCountry(c, cur)) {
        warningDiv.innerText = '✅ 地区货币匹配';
        warningDiv.style.cssText = 'color:#27ae60;background:#f0fff4;border-color:#c6f6d5;margin-top:12px;padding:8px 12px;border-radius:8px;font-size:11px;border-style:solid;border-width:1px;';
      } else {
        warningDiv.innerText = '⚠️ 货币与地区不匹配（可能导致支付失败）';
        warningDiv.style.cssText = 'color:#e74c3c;background:#fff5f5;border-color:#fed7d7;margin-top:12px;padding:8px 12px;border-radius:8px;font-size:11px;border-style:solid;border-width:1px;';
      }
    }

    countrySelect.onchange = () => {
      currencySelect.value = getCountryDefaultCurrency(countrySelect.value);
      updateWarning();
    };
    currencySelect.onchange = updateWarning;
    updateWarning();

    // 面板显示/隐藏
    let visible = false;
    fab.onclick = (e) => {
      e.stopPropagation();
      visible = !visible;
      panel.classList.toggle('show', visible);
    };
    document.getElementById('tm-close-panel-btn').onclick = () => {
      visible = false;
      panel.classList.remove('show');
    };
    document.addEventListener('click', (e) => {
      if (visible && !panel.contains(e.target) && !fab.contains(e.target)) {
        visible = false;
        panel.classList.remove('show');
      }
    });
    panel.onclick = (e) => e.stopPropagation();

    // 日志工具函数
    function clearLog() {
      logDiv.style.display = 'none';
      logDiv.style.background = 'transparent';
      logDiv.style.border = 'none';
      logDiv.innerText = '';
    }

    function showError(txt) {
      logDiv.style.display = 'block';
      logDiv.style.background = '#fff5f5';
      logDiv.style.border = '1px solid #fed7d7';
      logDiv.style.color = '#e53e3e';
      logDiv.innerText = '❌ ' + txt;
      if (!visible) { visible = true; panel.classList.add('show'); }
    }

    function showSuccess(url) {
      logDiv.style.display = 'block';
      logDiv.style.background = '#f0fff4';
      logDiv.style.border = '1px solid #c6f6d5';
      logDiv.style.color = '#27ae60';
      logDiv.innerHTML = '✅ 生成成功！<br><a href="' + url + '" target="_blank">👉 点击打开支付页面</a>';
      if (!visible) { visible = true; panel.classList.add('show'); }
      // 尝试用 GM_openInTab 打开（更可靠），回退到 window.open
      try {
        if (typeof GM_openInTab === 'function') {
          GM_openInTab(url, { active: true });
        } else {
          window.open(url, '_blank');
        }
      } catch (e) {
        window.open(url, '_blank');
      }
    }

    // 核心：生成支付链接
    execBtn.onclick = async () => {
      execBtn.disabled = true;
      execBtn.innerText = '⏳ 生成中...';
      clearLog();

      const country = countrySelect.value;
      const currency = currencySelect.value;
      const teamName = document.getElementById('tm-ws-name').value.trim() || 'MyTeam';
      const promo = document.getElementById('tm-promo-id').value.trim();
      const seatCount = parseInt(document.getElementById('tm-seat-count').value, 10) || 2;

      try {
        // 第一步：获取登录 session
        const sessResp = await fetch('/api/auth/session', {
          credentials: 'include'
        });
        if (!sessResp.ok) {
          throw new Error('未登录或会话已过期，请先登录 ChatGPT（状态码: ' + sessResp.status + '）');
        }
        const session = await sessResp.json();
        if (!session || !session.accessToken) {
          throw new Error('未获取到登录令牌，请刷新页面后重试');
        }

        // 第二步：构造 checkout payload
        const payload = {
          entry_point: "team_workspace_purchase_modal",
          plan_name: "chatgptteamplan",
          team_plan_data: {
            workspace_name: teamName,
            price_interval: "month",
            seat_quantity: seatCount
          },
          billing_details: {
            country: country,
            currency: currency
          },
          checkout_ui_mode: "hosted",
          cancel_url: "https://chatgpt.com/"
        };

        // 如果有优惠码则加入
        if (promo) {
          payload.promo_code = promo;
        }

        // 第三步：请求 checkout 接口
        const res = await fetch('/backend-api/payments/checkout', {
          method: 'POST',
          credentials: 'include',
          headers: {
            'Authorization': 'Bearer ' + session.accessToken,
            'Content-Type': 'application/json'
          },
          body: JSON.stringify(payload)
        });

        const data = await res.json();

        if (!res.ok) {
          const errMsg = data.detail || data.message || data.error || JSON.stringify(data);
          throw new Error('接口返回错误 (' + res.status + '): ' + errMsg);
        }

        // 第四步：提取支付链接
        let link = '';
        if (data.url) {
          link = data.url;
        } else if (data.checkout_session_id) {
          link = 'https://chatgpt.com/checkout/openai_llc/' + data.checkout_session_id;
        } else {
          throw new Error('接口返回数据中未找到支付链接: ' + JSON.stringify(data));
        }

        showSuccess(link);
      } catch (err) {
        showError(err.message || '未知错误');
      } finally {
        execBtn.disabled = false;
        execBtn.innerText = '🔗 生成支付链接';
      }
    };
  }

  // 延迟初始化，等待页面加载完成
  if (document.readyState === 'complete') {
    setTimeout(initUI, 800);
  } else {
    window.addEventListener('load', () => setTimeout(initUI, 800));
  }
})();
