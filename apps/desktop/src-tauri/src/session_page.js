// Observe ordinary visible DOM only. No credentials, network calls, decryption or purchases.
(() => {
  const platform='__PLATFORM__';
  const fail=error=>({error});
  try {
    if(document.readyState!=='complete')return fail('loading');
    const visible=e=>{if(!e||!e.getClientRects().length)return false;for(let n=e;n;n=n.parentElement){const s=getComputedStyle(n);if(n.hidden||n.getAttribute('aria-hidden')==='true'||['none'].includes(s.display)||['hidden','collapse'].includes(s.visibility)||s.opacity==='0'||s.contentVisibility==='hidden'||/blur\([1-9]/.test(s.filter))return false;}return true;};
    const text=e=>{if(!visible(e))return '';const walk=n=>n.nodeType===3?n.textContent:n.nodeType===1&&visible(n)&&!(platform==='ciweimao'&&n.matches('.J_Num'))&&! /^(SCRIPT|STYLE|BUTTON|INPUT|TEXTAREA|SELECT)$/.test(n.tagName)?Array.from(n.childNodes).map(walk).join(n.tagName==='BR'?'\n':''):'';return walk(e).trim();};
    const get=s=>text(document.querySelector(s));
    if(document.body.classList.contains('zh-captcha-page')||Array.from(document.querySelectorAll('iframe,[role="dialog"],.geetest_panel,.yidun_panel,#captcha-container,#tcaptcha_transform_dy,#J_ManMachineVerify')).some(e=>visible(e)&&/captcha|turing|验证|滑块|拖动/i.test((e.title||'')+(e.src||'')+text(e)+(e.id||'')+(e.className||''))))return fail('verification');
    const u=new URL(location.href);let title='',info='',body=null,declared=0;
    if(platform==='zongheng'&&u.hostname==='read.zongheng.com'){
      const m=u.pathname.match(/^\/chapter\/(\d+)\/(\d+)\.html$/);
      if(!m||document.body.dataset.bookid!==m[1]||document.body.dataset.chapterid!==m[2])return fail('identity');
      title=get('#Jcontent .title_txtbox');info=get('#Jcontent .bookinfo');body=document.querySelector('#Jcontent .reader-box .content');
    }else if(platform==='17k'&&u.hostname==='www.17k.com'){
      const m=u.pathname.match(/^\/chapter\/(\d+)\/(\d+)\.html$/),b=globalThis.$__BOOK;
      if(!m||String(b?.bookId)!==m[1]||String(b?.chapterId)!==m[2])return fail('identity');
      title=get('#readArea .readAreaBox > h1');info=get('#readArea .chapter_update_time');body=document.querySelector('#readArea .readAreaBox > .p');
    }else if(platform==='qimao'&&u.hostname==='www.qimao.com'){
      if(!/^\/shuku\/\d+-\d+\/$/.test(u.pathname))return fail('identity');
      title=get('.chapter-detail-wrap .chapter-title');info=get('.chapter-tips');body=document.querySelector('.chapter-detail-article > .article');
    }else if(platform==='shuqi'&&u.hostname==='www.shuqi.com'){
      if(u.pathname!=='/reader')return fail('identity');
      const data=JSON.parse(document.querySelector('.js-dataChapters')?.textContent||'null');
      if(String(data?.bookId)!==u.searchParams.get('bid'))return fail('identity');
      const row=data.chapterList?.flatMap(v=>v.volumeList||[]).find(c=>String(c.chapterId)===u.searchParams.get('cid'));
      if(!row)return fail('identity');
      const candidates=Array.from(document.querySelectorAll('.chapter-content')).filter(e=>text(e.querySelector('h3.chapter-title'))===String(row.chapterName).trim());
      if(candidates.length!==1)return fail('reader');
      body=candidates[0];title=text(body.querySelector('h3.chapter-title'));declared=Number(row.wordCount);
    }else if(platform==='jinjiang'&&['my.jjwxc.net','www.jjwxc.net'].includes(u.hostname)){
      if(!['/onebook_vip.php','/onebook.php'].includes(u.pathname))return fail('identity');
      for(const key of ['novelid','chapterid'])if(document.querySelector(`input[name="${key}"]`)?.value!==u.searchParams.get(key))return fail('identity');
      title=get('.noveltext h2');info=get('.noveltext .readsmall')||get('.noveltext .chapterinfo');body=document.querySelector('.noveltext > #paragraph_comment_content');
    }else if(platform==='ciweimao'&&u.hostname==='www.ciweimao.com'){
      const m=u.pathname.match(/^\/chapter\/(\d+)$/),b=globalThis.HB?.book;
      if(!m||String(b?.chapter_id)!==m[1]||String(b?.book_id)!=='__BOOK__')return fail('identity');
      if(visible(document.getElementById('J_BookImage')))return fail('protected');
      title=get('#J_BookCnt h1.chapter');info=get('#J_BookCnt .read-hd');body=document.querySelector('#J_BookCnt .read-bd');
    }else if(platform==='faloo'&&u.hostname==='b.faloo.com'){
      const m=u.pathname.match(/^\/(\d+)_(\d+)\.html$/);
      if(!m||document.getElementById('hidNovelID')?.value!==m[1]||document.getElementById('hidNodeID')?.value!==m[2])return fail('identity');
      title=get('.c_left h1').replace(get('#novelName'),'').trim();info=get('.c_left .readInfo')||get('.c_left .chapterInfo');body=document.querySelector('.c_left .noveContent');
    }else return fail('unsupported');
    if(Array.from(document.querySelectorAll('.paywall,.pay-wall,.subscribe-wrap,.trial-end,.vip-content,.chapter-pay')).some(visible))return fail('restricted');
    if(!body||!visible(body))return fail('reader');
    if(/订阅本章|购买本章|解锁本章|试读结束|本章为付费章节|本章为VIP章节|登录后阅读/.test(text(body)))return fail('restricted');
    if(Array.from(body.querySelectorAll('.loading,.chapter-loading,[aria-busy="true"]')).some(visible))return fail('loading');
    const illustration=e=>{if(platform!=='ciweimao'||e.tagName!=='IMG')return false;try{const image=new URL(e.src);return image.protocol==='https:'&&image.hostname==='e1.kuangxiangit.com'&&image.pathname.startsWith('/uploads/chapterimgsnew/')&&new RegExp('-__BOOK__-'+u.pathname.split('/').at(-1)+'\\.(jpg|png|jpeg|webp)$','i').test(image.pathname);}catch{return false;}};
    if(Array.from(body.querySelectorAll('img,canvas,iframe,svg')).some(e=>visible(e)&&!illustration(e)))return fail('protected');
    const wordMatch=info.match(/(?:本章字数|字数|本章共)\s*[:：]?\s*([\d,]+)/);if(!declared&&wordMatch)declared=Number(wordMatch[1].replaceAll(',',''));
    let visiblePageEnd;
    if(!Number.isSafeInteger(declared)||declared<=0){
      if(platform!=='faloo')return fail('word_count');
      // Faloo's observed ordinary reader has no declared chapter word count.
      // Require its visible end navigation after the complete paragraph container;
      // never fabricate a platform count from the text we just extracted.
      const catalog=document.getElementById('huimulu'),next=document.getElementById('next_page');
      if(!visible(catalog)||!visible(next)||!(body.compareDocumentPosition(catalog)&4)||!(body.compareDocumentPosition(next)&4))return fail('word_count');
      if(!/^目录$/.test(text(catalog))||!/^下一章\s*$/.test(text(next)))return fail('word_count');
      if(Array.from(body.children).some(e=>e.tagName!=='P'||!visible(e)||e.children.length))return fail('protected');
      visiblePageEnd={catalog_url:catalog.href,next_url:next.href};declared=0;
    }
    const skip=e=>e.matches('.copy,.author-say,.author_say,.chapter_text_ad,.qrcode,.watermark,[class*="watermark"]')||e.closest('.author-say,.author_say');
    let paragraphs=Array.from(body.querySelectorAll(':scope > p')).filter(e=>visible(e)&&!skip(e)).map(text).filter(Boolean);
    if(platform==='ciweimao'){
      const rows=Array.from(body.querySelectorAll(':scope > p.chapter')).filter(e=>visible(e)&&!skip(e));
      const marks=rows.flatMap(e=>Array.from(e.children).filter(n=>n.tagName==='SPAN').map(text));
      // Observed ordinary DOM appends one identical ASCII watermark per paragraph.
      // Unknown inline markup is rejected rather than silently dropping real prose.
      const marker=marks[0];
      if(marks.length&&(!marker||! /^[A-Za-z0-9]{6}$/.test(marker)||marks.some(m=>m!==marker)||marks.length!==rows.length))return fail('protected');
      if(rows.some(e=>Array.from(e.children).some(n=>!n.matches('.J_Num')&&!(n.tagName==='SPAN'&&text(n)===marker)&&!illustration(n))))return fail('protected');
      paragraphs=rows.map(e=>Array.from(e.childNodes).map(n=>n.nodeType===3?n.textContent:illustration(n)?'〔本段含官网插图，TXT 未保存图片〕':'').join('').trim()).filter(Boolean);
    }
    if(platform==='jinjiang'){
      if(Array.from(body.children).some(e=>e.tagName!=='BR'))return fail('protected');
      paragraphs=Array.from(body.childNodes).map(n=>n.nodeType===3?n.textContent:'\n').join('').split(/\n+/).map(s=>s.trim()).filter(Boolean);
    }
    if(!title||!paragraphs.length)return fail('reader');
    if(paragraphs.length>10000)return fail('large');
    const keys=platform==='shuqi'?['bid','cid']:platform==='jinjiang'?['novelid','chapterid']:[];
    const values=keys.map(k=>[k,u.searchParams.get(k)]);u.search='';u.hash='';for(const [k,v] of values)u.searchParams.set(k,v);
    return {url:u.href,title,paragraphs,declared_words:declared,...(visiblePageEnd?{visible_page_end:visiblePageEnd}:{})};
  }catch{return fail('unsupported');}
})();
