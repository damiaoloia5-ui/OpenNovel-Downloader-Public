(() => {
  const platform = '__PLATFORM__';
  const result = {platform, url: location.origin + location.pathname, status: 'unsupported', book_id: '', chapter_id: '', book_title: '', chapter_title: '', paragraphs: [], declared_words: null};
  const finish = status => ({...result, status, paragraphs: status === 'text' ? result.paragraphs : []});
  try {
    if (document.readyState !== 'complete') return finish('loading');
    const shown = node => {
      if (!node || !node.getClientRects().length) return false;
      for (let e = node; e; e = e.parentElement) {
        const s = getComputedStyle(e);
        if (s.display === 'none' || s.visibility === 'hidden' || s.visibility === 'collapse' || s.opacity === '0' || s.contentVisibility === 'hidden' || /blur\([1-9]/.test(s.filter)) return false;
      }
      return true;
    };
    const text = node => {
      if (!shown(node)) return '';
      const walk = e => e.nodeType === 3 ? e.textContent : e.nodeType === 1 && shown(e) && !/^(SCRIPT|STYLE|BUTTON|INPUT|TEXTAREA|SELECT)$/.test(e.tagName) ? Array.from(e.childNodes).map(walk).join('') : '';
      return walk(node).trim();
    };
    const first = selector => text(document.querySelector(selector));
    const meta = name => document.querySelector(`meta[property="${name}"],meta[name="${name}"]`)?.content?.trim() || '';
    const numeric = n => /^[1-9]\d{0,23}$/.test(String(n));
    for (const e of document.querySelectorAll('iframe,[role="dialog"],.geetest_panel,.yidun_panel,#captcha-container,#tcaptcha_transform_dy')) {
      if (shown(e) && /captcha|turing|验证|滑块|拖动/.test((e.title || '') + (e.src || '') + text(e) + (e.id || '') + (e.className || ''))) return finish('verification');
    }
    let reader = null;
    if (platform === 'ciweimao' && location.hostname === 'www.ciweimao.com') {
      const book = location.pathname.match(/^\/book\/([1-9]\d*)\/?$/);
      if (book) { result.book_id = book[1]; result.book_title = meta('og:novel:book_name'); return finish(result.book_title ? 'book' : 'unsupported'); }
      const chapter = location.pathname.match(/^\/chapter\/([1-9]\d*)$/);
      if (!chapter) return finish('unsupported');
      // Only public chapter identity fields; never access HB.userinfo or licenses.
      const identity = globalThis.HB?.book;
      if (!numeric(identity?.book_id) || String(identity?.chapter_id) !== chapter[1]) return finish('unsupported');
      result.book_id = String(identity.book_id); result.chapter_id = chapter[1];
      result.book_title = first('#J_BookCnt .read-hd p span');
      result.chapter_title = first('#J_BookCnt h1.chapter');
      if (shown(document.getElementById('J_BookImage'))) return finish('protected');
      reader = document.querySelector('#J_BookCnt .read-bd');
      const words = first('#J_BookCnt .read-hd').match(/字数\s*[:：]\s*([\d,]+)/);
      if (words) result.declared_words = Number(words[1].replaceAll(',', ''));
    } else if (platform === 'faloo' && location.hostname === 'b.faloo.com') {
      const book = location.pathname.match(/^\/([1-9]\d*)\.html$/);
      if (book) { result.book_id = book[1]; result.book_title = meta('og:novel:book_name') || meta('og:title'); return finish(result.book_title ? 'book' : 'unsupported'); }
      const chapter = location.pathname.match(/^\/([1-9]\d*)_([1-9]\d*)\.html$/);
      if (!chapter || document.getElementById('hidNovelID')?.value !== chapter[1] || document.getElementById('hidNodeID')?.value !== chapter[2]) return finish('unsupported');
      result.book_id = chapter[1]; result.chapter_id = chapter[2];
      result.book_title = first('#novelName');
      result.chapter_title = first('.c_left h1').replace(result.book_title, '').trim();
      reader = document.querySelector('.c_left .noveContent');
    } else if (platform === 'migu' && location.hostname === 'www.cmread.com') {
      const book = location.pathname.match(/^\/yuekan\/novel\/([1-9]\d*)\.html$/);
      if (book) { result.book_id = book[1]; result.book_title = first('h1'); return finish(result.book_title ? 'book' : 'unsupported'); }
      return finish('unsupported');
    } else return finish('unsupported');
    if (!reader || !shown(reader)) return finish('unsupported');
    if (/订阅本章|购买本章|解锁本章|试读结束|本章为付费章节|本章为VIP章节/.test(text(reader)) || Array.from(document.querySelectorAll('.paywall,.pay-wall,.subscribe-wrap,.trial-end')).some(shown)) return finish('restricted');
    if (reader.querySelector('img,canvas,iframe,svg')) return finish('protected');
    result.paragraphs = Array.from(reader.querySelectorAll('p')).filter(shown).map(text).filter(Boolean);
    if (!result.book_title || !result.chapter_title || !result.paragraphs.length) return finish('unsupported');
    if (result.paragraphs.length > 10000 || result.paragraphs.join('').length > 200000) return finish('unsupported');
    return finish('text');
  } catch { return finish('unsupported'); }
})();
