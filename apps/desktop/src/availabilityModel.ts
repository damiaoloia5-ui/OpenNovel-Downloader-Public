import type {Catalog,SourceInfo,ReadingChapter} from './api';

// A display hint only; the backend independently validates the complete URL.
export function sourceHint(input:string,sources:readonly SourceInfo[]):SourceInfo|undefined {
  try {
    const url=new URL(input.trim());
    if(url.protocol!=='https:'||url.username||url.password||url.port)return;
    return sources.find(source=>{try{return new URL(source.example).hostname===url.hostname;}catch{return false;}});
  } catch {return;}
}

export function catalogAvailability(catalog:Catalog,sources:readonly SourceInfo[]) {
  const supported=sources.some(s=>s.id===catalog.source&&s.can_download);
  const positions=catalog.chapters.flatMap((c,i)=>supported&&!c.restricted?[i+1]:[]);
  const ranges:string[]=[];
  for(let i=0;i<positions.length;i++){
    const first=positions[i]!;let last=first;
    while(i+1<positions.length&&positions[i+1]===last+1)last=positions[++i]!;
    ranges.push(first===last?`${first}`:`${first}-${last}`);
  }
  return {supported,total:catalog.chapters.length,open:positions.length,
    restricted:supported?catalog.chapters.filter(c=>c.restricted).length:0,
    unverified:supported?0:catalog.chapters.length,selection:ranges.join(',')};
}

export const availabilityLabels:Record<string,string>={saved:'已保存',restricted:'访问受限',failed:'获取失败',pending:'等待保存',paused:'任务暂停',cancelled:'任务取消',unattempted:'尚未尝试',unsupported:'来源未开放',unverified:'状态待核对'};
export const availabilityAdvice:Record<string,string>={restricted:'先核对官网提示、章节开放状态与本人权益；仅凭失败不能判断是付费还是登录问题。',failed:'查看下面的原始原因，可到任务明细重试允许重试的失败章。',pending:'等待原任务完成，可在保存正文页查看进度或暂停。',paused:'在保存正文页继续原任务，已保存内容会复用。',cancelled:'可在保存正文页核对原任务并明确继续。',unattempted:'查看此前选章范围；已支持网页可检查目录补缺章，账户来源需先核对官网。',unsupported:'当前适配器尚未开放正文，不会自动切换其他来源。',unverified:'刷新本书状态并检查任务记录，暂不推断远端权限。'};
export function chapterEvidence(chapter:ReadingChapter){
  if(chapter.saved)return {state:'saved',reason:'本地已有正文，可离线阅读与导出',job_id:null};
  return chapter.availability||{state:'unverified',reason:'暂无逐章依据，请刷新本书状态',job_id:null};
}
export function missingGroups(chapters:readonly ReadingChapter[]){
  const counts=new Map<string,number>();
  for(const chapter of chapters){if(chapter.saved)continue;const state=chapterEvidence(chapter).state;counts.set(state,(counts.get(state)||0)+1);}
  return [...counts].map(([state,count])=>({state,count,label:availabilityLabels[state]||'状态待核对',advice:availabilityAdvice[state]||availabilityAdvice.unverified}));
}
