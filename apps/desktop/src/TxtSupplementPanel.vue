<script setup lang="ts">
import {ref} from 'vue';
import {previewTxtSupplement,confirmTxtSupplement,type TxtSupplementPreview} from './api';
const props=defineProps<{bookId:string;disabled:boolean}>();
const emit=defineEmits<{saved:[]}>();
const preview=ref<TxtSupplementPreview|null>(null);const text=ref('');const filename=ref('');const busy=ref(false);const error=ref('');const notice=ref('');
async function choose(event:Event){
  preview.value=null;text.value='';error.value='';notice.value='';
  const file=(event.target as HTMLInputElement).files?.[0];if(!file)return;
  filename.value=file.name;if(file.size>16*1024*1024){error.value='TXT 超过 16 MiB，请选择较小文件。';return;}
  busy.value=true;
  try{
    const owner=props.bookId;
    const value=new TextDecoder('utf-8',{fatal:true}).decode(await file.arrayBuffer());
    const result=await previewTxtSupplement(owner,value);
    if(owner===props.bookId){text.value=value;preview.value=result;}
  }catch(e){error.value=`无法匹配 UTF-8 TXT：${String(e)}`;}finally{busy.value=false;}
}
async function confirm(){
  if(!preview.value||busy.value||props.disabled)return;
  busy.value=true;error.value='';
  try{const result=await confirmTxtSupplement(props.bookId,text.value,preview.value.fingerprint);notice.value=`已补齐 ${result.total-result.already_saved} 章，原有正文保留。TXT 将自动更新。`;preview.value=null;text.value='';emit('saved');}
  catch(e){error.value=String(e);preview.value=null;text.value='';}finally{busy.value=false;}
}
</script>

<template>
  <section class="txt-supplement" aria-label="从已有 TXT 补齐本书">
    <h3>已有完整 TXT？直接补齐本书</h3>
    <p>选择你从成品下载器或其他正常渠道取得的同书 UTF-8 TXT。会核对全部章节标题与顺序，仅补齐缺章，原有正文保留。书名可能改名，请自行确认文件确为本书、正文完整。</p>
    <label>选择同书完整 TXT<input type="file" accept=".txt,text/plain" :disabled="disabled||busy" @change="choose" /></label>
    <p v-if="busy" role="status">正在核对或保存，请稍候……</p>
    <p v-if="error" role="alert">{{ error }}</p><p v-if="notice" role="status">{{ notice }}</p>
    <div v-if="preview">
      <p>{{ filename }} → {{ preview.title }}</p>
      <p>匹配 {{ preview.total }} 章；已有 {{ preview.already_saved }} 章，将补齐 {{ preview.total-preview.already_saved }} 章。</p>
      <p>首章：{{ preview.first_title }}<br />末章：{{ preview.last_title }}</p>
      <p>标题匹配不能证明正文完整，请核对文件后确认。原始文件不会改动。</p>
      <button class="primary" :disabled="disabled||busy" @click="confirm">确认是同一本书，补齐并自动生成 TXT</button>
    </div>
  </section>
</template>

<style scoped>
.txt-supplement{margin:22px 0;padding:20px;border:1px solid #bdc9b1;border-radius:8px;background:#f3f6ed}.txt-supplement h3{margin-top:0}.txt-supplement label{display:grid;gap:8px}.txt-supplement p{line-height:1.8}.txt-supplement input{max-width:100%}[role=alert]{color:#9a3024}
</style>
