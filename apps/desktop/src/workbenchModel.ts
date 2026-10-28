import type {Book,Job,SourceInfo} from './api';

export type WorkbenchTab='overview'|'save'|'read'|'export';
export type SaveMode='account'|'public'|'local'|'unavailable';
export interface WorkbenchStatus {kind:string;title:string;message:string;action:string;tab:WorkbenchTab;job?:Job}

export function saveMode(source:string,sources:readonly SourceInfo[]):SaveMode {
  if(source==='local-json')return 'local';
  if(source==='qidian-session')return 'account';
  return sources.some(s=>s.id===source&&s.can_download)?'public':'unavailable';
}

// Whole-book completeness always comes from the book, never from a selected task.
export function workbenchStatus(book:Book,jobs:readonly Job[],mode:SaveMode):WorkbenchStatus {
  const own=jobs.filter(j=>j.book_id===book.id);
  const running=own.find(j=>['running','queued'].includes(j.state));
  if(running)return {kind:'running',title:'正在保存',message:'可以在这里查看进度或暂停，已保存正文可随时阅读和导出。',action:'查看保存进度',tab:'save',job:running};
  const paused=own.find(j=>j.state==='paused');
  if(paused)return {kind:'paused',title:'已暂停，进度保留',message:mode==='account'?'打开原账户官网窗口并核对登录后，继续原任务。':'继续原任务会复用已经保存的正文。',action:'继续保存',tab:'save',job:paused};
  if(book.total_chapters>0&&book.saved_chapters===book.total_chapters)return {kind:'complete',title:'正文已保存完整',message:'后台会自动生成 TXT，可在本页打开文件夹；也可离线阅读或手动导出其他格式。',action:'查看文件与其他导出',tab:'export'};
  if(mode==='unavailable')return {kind:'unavailable',title:'当前来源未开放正文保存',message:'目录不等于正文。已有内容仍可阅读或导出，来源权限和支持范围保持不变。',action:book.saved_chapters?'阅读已保存正文':'查看支持说明',tab:book.saved_chapters?'read':'save'};
  const latest=own[0];
  if(latest&&['failed','partial','blocked','cancelled'].includes(latest.state))return {kind:'attention',title:latest.state==='cancelled'?'任务已取消，正文保留':'保存已停止，需要处理',message:latest.error||'查看任务原因后再继续。已保存内容不会丢失。',action:'查看原因与下一步',tab:'save',job:latest};
  if(mode==='local')return {kind:'local',title:'本地内容尚未收齐',message:'查看原导入任务，继续可恢复的任务，或检查原始 JSON 文件。',action:'查看导入进度',tab:'save',job:latest};
  return {kind:'missing',title:book.saved_chapters?'还有章节未保存':'目录已就绪，尚无正文',message:mode==='account'?'在本书的保存页打开官网并核对账户，然后保存缺章。':'保存缺章会复用已有正文；网页受限内容继续标注缺失。',action:book.saved_chapters?'保存缺章':'开始保存正文',tab:'save',job:latest};
}

export const exportFormatKey='opennovel.ui.export-format';
export function preferredExportFormat(storage:Pick<Storage,'getItem'>):'txt'|'epub' {
  try{return storage.getItem(exportFormatKey)==='epub'?'epub':'txt';}catch{return 'txt';}
}
export function rememberExportFormat(storage:Pick<Storage,'setItem'>,format:'txt'|'epub'):boolean {
  try{storage.setItem(exportFormatKey,format);return true;}catch{return false;}
}
