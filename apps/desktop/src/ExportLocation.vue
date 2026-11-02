<script setup lang="ts">
import {onMounted,onUnmounted,ref,watch} from 'vue';
import {bookExportDestination,chooseBookExportDirectory,setBookExportDirectory,type ExportDestination} from './api';
const props=defineProps<{bookId:string}>();
const destination=ref<ExportDestination|null>(null);const draft=ref('');const busy=ref(false);const error=ref('');const notice=ref('');
let sequence=0;let disposed=false;
async function load(){const owner=props.bookId;const ticket=++sequence;destination.value=null;error.value='';notice.value='';try{const value=await bookExportDestination(owner);if(!disposed&&owner===props.bookId&&ticket===sequence){destination.value=value;draft.value=value.directory;}}catch(e){if(!disposed&&owner===props.bookId&&ticket===sequence)error.value=String(e);}}
async function change(mode:'choose'|'typed'|'reset'){
  if(busy.value)return;const owner=props.bookId;const ticket=++sequence;busy.value=true;error.value='';notice.value='';
  try{const value=mode==='choose'?await chooseBookExportDirectory(owner):await setBookExportDirectory(owner,mode==='reset'?null:draft.value.trim());
    if(!disposed&&owner===props.bookId&&ticket===sequence&&value){destination.value=value;draft.value=value.directory;notice.value='已记住本书保存位置。TXT 会从本地正文重新生成；旧文件保留，之后的导出使用此目录。';}
  }catch(e){if(!disposed&&owner===props.bookId&&ticket===sequence)error.value=String(e);}finally{busy.value=false;}
}
watch(()=>props.bookId,()=>{void load();});onMounted(()=>{void load();});onUnmounted(()=>{disposed=true;sequence++;});
</script>
<template>
  <section class="export-location-setting" aria-label="本书保存位置">
    <div class="location-heading"><strong>本书保存位置</strong><button :disabled="busy" @click="change('choose')">{{ busy?'正在处理…':'选择文件夹' }}</button></div>
    <code v-if="destination">{{ destination.directory }}</code><p v-else-if="!error">正在读取保存位置…</p>
    <p v-if="destination" class="filename-preview">文件名：{{ destination.filename }}<br>TXT 和 EPUB 都包含作品信息与简介；同名旧版本保留并追加编号。</p>
    <details><summary>输入文件夹路径或恢复默认位置</summary><form @submit.prevent="change('typed')"><label>文件夹完整路径<input v-model="draft" :disabled="busy" placeholder="填写已存在的文件夹完整路径" /></label><button :disabled="busy||!draft.trim()">应用并记住</button><button type="button" :disabled="busy||!destination?.custom" @click="change('reset')">恢复本书默认目录</button></form></details>
    <p v-if="error" role="alert">{{ error }}</p><p v-if="notice" role="status">{{ notice }}</p>
  </section>
</template>
<style scoped>
.export-location-setting{padding:16px 0;border-top:1px solid var(--border);margin-top:16px}.location-heading{display:flex;align-items:center;justify-content:space-between;gap:12px;flex-wrap:wrap;font-size:13px}.export-location-setting code{display:block;white-space:pre-wrap;overflow-wrap:anywhere;margin:12px 0;font-size:12px}.export-location-setting p,.export-location-setting summary{font-size:12px;line-height:1.8;color:var(--muted)}.export-location-setting form{display:flex;flex-wrap:wrap;align-items:end;gap:10px;margin-top:12px}.export-location-setting label{display:grid;gap:8px;flex:1 1 280px;font-size:12px}.export-location-setting input{min-width:0;width:100%;padding:10px;border:1px solid var(--border);border-radius:5px;background:var(--surface-soft);color:var(--ink);font:inherit}.export-location-setting [role=alert]{color:var(--danger)}
</style>
