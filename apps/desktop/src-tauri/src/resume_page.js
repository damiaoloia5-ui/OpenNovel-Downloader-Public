// Observe visible challenge UI only. Never operate it or read credentials/body text.
(() => {
  try {
    if (document.readyState !== 'complete') return { status: 'loading' };
    const visible = element => {
      for (let node = element; node; node = node.parentElement) {
        const style = getComputedStyle(node);
        if (node.hidden || node.getAttribute('aria-hidden') === 'true' || style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse' || style.opacity === '0') return false;
      }
      return element.getClientRects().length > 0;
    };
    const widgets = '#tcaptcha_transform_dy,.tcaptcha-transform,.geetest_panel,.geetest_panel_box,.yidun_panel,#captcha-container,[role="dialog"]';
    if (Array.from(document.querySelectorAll(widgets)).some(e => visible(e) && (e.getAttribute('role') !== 'dialog' || /滑块|拖动.{0,20}验证|安全验证|人机验证|验证码/.test(e.innerText || '')))) return { status: 'verification' };
    if (Array.from(document.querySelectorAll('iframe')).some(e => visible(e) && /captcha|turing|验证码|人机验证|安全验证|滑块/i.test(`${e.title || ''} ${e.src || ''}`))) return { status: 'verification' };
    return { status: 'ready' };
  } catch { return { status: 'unknown' }; }
})();
