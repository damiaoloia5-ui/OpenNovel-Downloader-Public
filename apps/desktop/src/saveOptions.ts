export type SaveFormat='txt'|'docx';
export interface SaveOptions {range:'all'|'range';first:number;last:number|null;format:SaveFormat}
export function defaultSaveOptions():SaveOptions {return {range:'all',first:1,last:null,format:'txt'};}
export function resolveSaveRange(options:SaveOptions,total?:number):string {
  if(options.range==='all')return '';
  const {first,last}=options;
  if(!Number.isSafeInteger(first)||first<1||last===null||!Number.isSafeInteger(last)||last<first)throw new Error('请填写有效的起止章节：起始至少为 1，结束不能小于起始。');
  if(total!==undefined&&last>total)throw new Error(`结束章节超过目录总数（${total} 章），请修改范围。`);
  return `${first}-${last}`;
}
export function formatLabel(format:string):string {return format==='docx'?'Word':format.toUpperCase();}
