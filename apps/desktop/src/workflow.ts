// UI routing only. The core still validates every official address and permission.
export function officialBookPlatform(input:string):'ciweimao'|'faloo'|'migu'|null {
  try {
    const u=new URL(input.trim());
    if(u.protocol!=='https:'||u.port||u.username||u.password||u.search||u.hash)return null;
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

export function saveRecovery(error:string):string {
  if(/登录|会话.*关闭|窗口.*关闭/.test(error))return '在起点官网窗口重新登录，再回到这里继续保存。';
  if(/验证|验证码/.test(error))return '在官网窗口完成验证，再回到这里继续保存。';
  if(/订阅|付费|保护|权限/.test(error))return '在官网核对这一章能否正常阅读。当前无法保存时，可先打开已生成的 TXT。';
  if(/网络|超时|连接/.test(error))return '检查网络和官网是否正常，稍后点“继续保存”。';
  if(/目录|章数|作品|标题/.test(error))return '打开这本书的官网目录，展开全部章节后重新核对。';
  return '先查看官网窗口的提示，处理后再继续。已保存的正文不会丢失。';
}
