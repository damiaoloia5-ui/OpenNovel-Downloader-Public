// UI routing only. The core still validates every official address and permission.
export const sessionSources=['qidian-session','zongheng-session','17k-session','jinjiang-session','qimao-session','shuqi-session','ciweimao-session','faloo-session'];
export function isSessionSource(source:string):boolean{return sessionSources.includes(source);}
export function sessionBookLink(platform:string,id:string):string {
  if(!/^\d{1,30}$/.test(id))return '';
  return ({zongheng:`https://www.zongheng.com/detail/${id}`,'17k':`https://www.17k.com/book/${id}.html`,jinjiang:`https://www.jjwxc.net/onebook.php?novelid=${id}`,qimao:`https://www.qimao.com/shuku/${id}/`,shuqi:`https://www.shuqi.com/book/${id}.html`,ciweimao:`https://www.ciweimao.com/book/${id}`,faloo:`https://b.faloo.com/${id}.html`} as Record<string,string>)[platform]||'';
}
export function sessionBookPlatform(input:string):string|null {
  try{const u=new URL(input.trim());if(u.protocol!=='https:'||u.port||u.username||u.password||u.hash)return null;
    for(const p of ['zongheng','17k','jinjiang','qimao','shuqi','ciweimao','faloo']){
      const id=p==='jinjiang'?u.searchParams.get('novelid'):u.pathname.match(/(?:\/book\/|\/detail\/|\/shuku\/|\/)(\d+)(?:\.html|\/)?$/)?.[1];
      if(id&&sessionBookLink(p,id)===u.href)return p;
    }return null;
  }catch{return null;}
}
export function officialBookPlatform(input:string):'ciweimao'|'faloo'|'migu'|null {
  try {
    const u=new URL(input.trim());
    if(u.protocol!=='https:'||u.port||u.username||u.password||u.hash)return null;
    if(u.hostname==='n.cmread.com'&&u.pathname==='/nap/p/wap_detail_new.jsp'){
      const ids=u.searchParams.getAll('bid');
      return ids.length===1&&/^[1-9]\d{0,23}$/.test(ids[0]!)&&Array.from(u.searchParams.keys()).every(k=>['bid','type','cm'].includes(k))?'migu':null;
    }
    if(u.search)return null;
    if(u.hostname==='www.ciweimao.com'&&/^\/book\/[1-9]\d{0,23}\/?$/.test(u.pathname))return 'ciweimao';
    if(u.hostname==='b.faloo.com'&&/^\/[1-9]\d{0,23}\.html$/.test(u.pathname))return 'faloo';
    if(u.hostname==='www.cmread.com'&&/^\/yuekan\/novel\/[1-9]\d{0,23}\.html$/.test(u.pathname))return 'migu';
    return null;
  }catch{return null;}
}
export function qidianBookLink(input: string): string | null {
  try {
    const url = new URL(input.trim());
    if (url.protocol !== 'https:' || url.hostname !== 'www.qidian.com' || url.port || url.username || url.password || url.search || url.hash) return null;
    const match = url.pathname.match(/^\/book\/(\d+)(?:\/catalog)?\/?$/);
    return match ? `https://www.qidian.com/book/${match[1]}/` : null;
  } catch { return null; }
}

export function qidianBookId(input: string): string | null {
  return qidianBookLink(input)?.split('/')[4] || null;
}

export function saveRecovery(error:string,context:'account'|'public'|'engine'='account'):string {
  if(context==='engine'){
    if(/校验|标题.*不一致|未识别字形|异常结构/.test(error))return '本地引擎返回的内容未通过校验，本章未入库。已保存正文保留；请保留下面的具体原因用于排查，反复点击继续不会修复校验问题。';
    if(/限流|频繁|429/.test(error))return '来源暂时限制请求频率，已保留进度。请按提示等待后再继续保存。';
    if(/验证|登录|403|访问需处理/.test(error))return '来源暂时拒绝本地引擎访问，已保留进度。请稍后再试；此通道不使用官网登录窗口的会话。';
    if(/付费|权限|订阅/.test(error))return '来源未确认这一章免费，已停止保存。可以打开已生成的 TXT 查看已有内容。';
    if(/网络|超时|连接/.test(error))return '检查网络后继续保存，已有正文会自动跳过。';
    return '本地引擎已停止，已保存正文保留。请查看下面的具体原因；可在设置中检查本地组件状态。';
  }
  if(context==='public'&&/官网访问需处理|验证|登录/.test(error))return '官网当前拒绝程序访问，已停止后续请求并保留进度。请稍后继续；外部浏览器登录不等于程序已获得访问权限。';
  if(/登录|会话.*关闭|窗口.*关闭/.test(error))return '在当前平台官网窗口重新登录，再回到这里继续保存。';
  if(/验证|验证码/.test(error))return '在官网窗口完成验证，再回到这里继续保存。';
  if(/订阅|付费|保护|权限/.test(error))return '在官网核对这一章能否正常阅读。当前无法保存时，可先打开已生成的 TXT。';
  if(/网络|超时|连接/.test(error))return '检查网络和官网是否正常，稍后点“继续保存”。';
  if(/目录|章数|作品|标题/.test(error))return '打开这本书的官网目录，展开全部章节后重新核对。';
  return '先查看官网窗口的提示，处理后再继续。已保存的正文不会丢失。';
}
