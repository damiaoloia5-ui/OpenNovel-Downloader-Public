import type {CapturePreview} from './api';
import {qidianBookId,qidianBookLink} from './workflow.ts';

// Only transient catalog/loading failures may be polled. Account, permission,
// verification, size and unknown failures always return to the user.
const transient=new Set([
  '完整目录或官网总章数未识别；请展开目录并等待全部加载后重试',
  '当前页面结构暂不支持；请确认已打开完整目录或完整正文，未保存任何内容',
  '读取期间页面已切换，请重新预览',
]);
export async function readGuidedCatalog(input:string,probe:()=>Promise<CapturePreview>,current:()=>boolean,wait:()=>Promise<void>):Promise<CapturePreview|null> {
  const link=qidianBookLink(input);
  if(!link)throw new Error('请填写这本书的起点官网链接。');
  let last:unknown;
  for(let attempt=0;attempt<6;attempt++){
    if(!current())return null;
    await wait();
    if(!current())return null;
    try{
      const result=await probe();
      if(!current())return null;
      if(result.kind!=='catalog'||!result.catalog||result.catalog.remote_id!==qidianBookId(link))throw new Error('读到的不是这本书，请重新打开作品目录。');
      return result;
    }catch(error){
      if(!current())return null;
      if(!transient.has(error instanceof Error?error.message:String(error)))throw error;
      last=error;
    }
  }
  throw last;
}
