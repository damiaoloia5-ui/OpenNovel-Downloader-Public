// Read normally rendered DOM only. No network, storage, hidden state, or credential access.
(() => {
  try {
    const mode = '__MODE__';
    const cleanUrl = location.origin + location.pathname;
    const match = location.pathname.match(/^\/(book|chapter)\/(\d+)\/(?:(\d+|catalog)\/?)?$/);
    if (location.origin !== 'https://www.qidian.com' || !match) return { error: 'page' };
    const book = match[2];
    if (document.readyState !== 'complete') return { error: 'loading' };
    // Cache only within this synchronous extraction. Never reuse visibility
    // across polls: a subscription/verification overlay can appear later.
    const styleVisibility = new WeakMap();
    const stylesShown = node => {
      if (!node) return true;
      const cached = styleVisibility.get(node);
      if (cached !== undefined) return cached;
      const style = getComputedStyle(node);
      const own = !(node.hidden || node.getAttribute('aria-hidden') === 'true' || style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse' || style.opacity === '0' || /blur\(/.test(style.filter));
      const value = own && stylesShown(node.parentElement);
      styleVisibility.set(node, value);
      return value;
    };
    const shown = element => stylesShown(element) && element.getClientRects().length > 0;
    const text = node => {
      if (node.nodeType === Node.TEXT_NODE) return node.textContent || '';
      if (node.nodeType !== Node.ELEMENT_NODE || !shown(node)) return '';
      if (node.matches('script,style,button,svg,canvas,i,.review,.review-count,.para-comment-num,.para-comment,.review-link,.num-word-cut')) return '';
      if (node.tagName === 'BR') return '\n';
      return Array.from(node.childNodes, text).join('');
    };
    const first = (root, selectors) => {
      for (const selector of selectors) {
        const found = Array.from(root.querySelectorAll(selector)).find(shown);
        if (found) return text(found).trim();
      }
      return '';
    };
    if (Array.from(document.querySelectorAll('#tcaptcha_transform_dy,.tcaptcha-transform,.geetest_panel,.geetest_panel_box,.yidun_panel,#captcha-container,[role="dialog"]')).some(e => shown(e) && (e.getAttribute('role') !== 'dialog' || /滑块|拖动.{0,20}验证|安全验证|人机验证|验证码/.test(e.innerText || '')))) return { error: 'verification' };
    if (Array.from(document.querySelectorAll('iframe')).some(e => {
      const r = e.getBoundingClientRect();
      return shown(e) && r.bottom > 0 && r.right > 0 && r.top < innerHeight && r.left < innerWidth && /captcha|turing|验证码|人机验证|安全验证|滑块/i.test(`${e.title || ''} ${e.src || ''}`);
    })) return { error: 'verification' };
    if (mode === 'catalog') {
      if (match[1] !== 'book') return { error: 'catalog_page' };
      const anchors = document.querySelectorAll('#allCatalog .chapter-item a[href], .volume-wrap a[href], #j-catalogWrap a[href], .catalog-content a[href]');
      const chapters = [];
      const ids = new Map();
      for (const a of anchors) {
        if (!shown(a)) continue;
        const url = new URL(a.href, location.href);
        const path = url.pathname.match(/^\/chapter\/(\d+)\/(\d+)\/?$/);
        if (url.origin !== location.origin || !path || path[1] !== book) continue;
        const title = text(a).trim();
        if (!title) continue;
        if (ids.has(path[2])) {
          if (ids.get(path[2]) !== title) return { error: 'catalog' };
          continue;
        }
        ids.set(path[2], title);
        chapters.push({ id: path[2], title });
        if (chapters.length > 10000) return { error: 'large' };
      }
      const totalText = first(document, ['.catalog-header-desc']);
      const count = totalText ? totalText.match(/共\s*([\d,]+)\s*章/) : document.body.innerText.match(/(?:目录|全部章节)[\s（(【]*(?:共\s*)?([\d,]+)\s*章/);
      const declared_total = count ? Number(count[1].replaceAll(',', '')) : 0;
      const title = first(document, ['#bookName', '.book-info h1 em', '.book-info h1', 'h1[itemprop="name"]', 'h1']);
      const author = first(document, ['.book-meta .author', '.book-info .writer', '.book-info [itemprop="author"]', '.writer']).replace(/^作者\s*[:：]\s*/, '').replace(/\s*著\s*$/, '');
      if (!declared_total || declared_total !== chapters.length || !title) return { error: 'catalog' };
      return { url: cleanUrl, title, author, declared_total, chapters };
    }
    if (match[1] !== 'chapter' || !match[3]) return { error: 'chapter_page' };
    const chapter = match[3].replace('/', '');
    if (Array.from(document.querySelectorAll('button')).some(e => shown(e) && e.innerText.trim() === '订阅本章')) return { error: 'restricted' };
    const modernReader = document.getElementById('c-' + chapter);
    const modernOwner = modernReader?.closest('[data-id="' + chapter + '"]');
    const owner = modernOwner || document.getElementById('chapter-' + chapter) || document.querySelector('.main-text-wrap[data-cid="' + chapter + '"]');
    const readers = Array.from((owner || document).querySelectorAll('.read-content, .chapter-content, main.content[id="c-' + chapter + '"]')).filter(shown);
    if (readers.length !== 1) return { error: 'reader' };
    const reader = readers[0];
    const scope = owner || reader.closest('.main-text-wrap, [data-cid], article, .chapter-wrapper') || reader.parentElement;
    const scopeId = scope.getAttribute('data-cid') || scope.getAttribute('data-id');
    if (scopeId && scopeId !== chapter) return { error: 'reader' };
    const gates = scope.querySelectorAll('.subscribe-btn,.subscribe-wrap,.subscribe-box,.pay-wall,.paywall,.trial-end');
    if (Array.from(gates).some(shown) || /订阅本章|本章为付费章节|本章为VIP章节|试读结束|试看结束/.test(scope.innerText)) return { error: 'restricted' };
    if (reader.querySelector('canvas,iframe')) return { error: 'protected' };
    const title = first(scope, ['h1.title', '.j_chapterName', '.chapter-title', '.text-head h3', 'h1', 'h2', 'h3']);
    const modernMetadata = modernOwner?.querySelector('h1.title')?.nextElementSibling;
    const metadata = modernMetadata && shown(modernMetadata) ? text(modernMetadata) : first(scope, ['.text-info', '.chapter-info', '.chapter-meta']);
    const words = metadata.match(/([\d,]+)\s*字/);
    const paragraphs = Array.from(reader.querySelectorAll('p')).filter(shown).flatMap(p => text(p).split(/\r?\n/)).map(p => p.trim()).filter(Boolean);
    if (!title || !words || !paragraphs.length) return { error: 'reader' };
    const declared_words = Number(words[1].replaceAll(',', ''));
    if (paragraphs.length > 10000 || paragraphs.join('').length > 400000) return { error: 'large' };
    return { url: cleanUrl, title, paragraphs, declared_words };
  } catch {
    return { error: 'reader' };
  }
})();
