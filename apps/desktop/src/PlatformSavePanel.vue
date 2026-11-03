<script setup lang="ts">
import {computed,onMounted,onUnmounted,ref,watch} from 'vue';
import AutoSavePanel from './AutoSavePanel.vue';
import SaveLocation from './SaveLocation.vue';
import {accountList,createAccount,openAccount,previewPlatformCatalog,confirmAccountPage,startAccountDownload,snapshot,type CapturePreview,type Book} from './api';
import {sessionBookLink} from './workflow';
const props=defineProps<{initialUrl?:string;bookId?:string;initialDirectory?:string|null}>();
const emit=defineEmits<{saved:[bookId:string];read:[bookId:string]}>();
const platform=ref('zongheng'),url=ref(''),account=ref(''),book=ref<Book|null>(null),preview=ref<CapturePreview|null>(null),busy=ref(false),opened=ref(false),ack=ref(false),error=ref('');let epoch=0,disposed=false;
const directory=ref<string|null>(props.initialDirectory??null);const choosingDirectory=ref(false);const directoryTouched=ref(props.initialDirectory!==undefined);
const names:Record<string,string>={zongheng:'纵横', '17k':'17K',jinjiang:'晋江',qimao:'七猫',shuqi:'书旗',ciweimao:'刺猬猫',faloo:'飞卢'};
const source=computed(()=>`${platform.value}-session`);
function inferPlatform(link:string){try{const host=new URL(link).hostname;return ({'www.zongheng.com':'zongheng','www.17k.com':'17k','www.jjwxc.net':'jinjiang','www.qimao.com':'qimao','www.shuqi.com':'shuqi','www.ciweimao.com':'ciweimao','b.faloo.com':'faloo'} as Record<string,string>)[host];}catch{return undefined;}}
async function restore(){const own=++epoch,initial=props.initialUrl,id=props.bookId;preview.value=null;book.value=null;account.value='';opened.value=false;ack.value=false;error.value='';try{const state=await snapshot();if(!valid(own))return;const old=state.books.find(b=>b.id===id&&b.source.endsWith('-session')&&names[b.source.replace('-session','')]);if(old){platform.value=old.source.replace('-session','');url.value=sessionBookLink(platform.value,old.remote_id);book.value=old;}else if(initial){platform.value=inferPlatform(initial)||platform.value;url.value=initial;}else url.value='';}catch(e){if(valid(own))error.value=String(e);}}
async function run(work:(own:number)=>Promise<void>){if(busy.value)return;const own=epoch;busy.value=true;error.value='';try{await work(own);}catch(e){if(!disposed&&own===epoch)error.value=String(e);}finally{if(!disposed)busy.value=false;}}
function valid(own:number){return !disposed&&epoch===own;}
watch([platform,url],()=>{epoch++;preview.value=null;book.value=null;opened.value=false;account.value='';ack.value=false;error.value='';},{flush:'sync'});
watch(()=>[props.initialUrl,props.bookId],()=>void restore());
async function ensureAccount(p:string,own:number){const records=await accountList();if(!valid(own))return null;const existing=records.find(r=>r.account.platform===p);return existing?.account.id||(await createAccount(p,`我的${names[p]}账户`)).id;}
function login(){void run(async own=>{const p=platform.value;const id=await ensureAccount(p,own);if(!id||!valid(own))return;await openAccount(id,'',true);if(valid(own)&&p===platform.value){account.value=id;opened.value=true;}});}
function catalog(){void run(async own=>{preview.value=null;ack.value=false;const result=await previewPlatformCatalog(account.value,url.value);if(valid(own))preview.value=result;});}
function start(){void run(async own=>{if(!ack.value||!preview.value)throw new Error('请核对目录和阅读权限');const result=await confirmAccountPage(account.value,preview.value.token);if(!valid(own))return;preview.value=null;const state=await snapshot();if(!valid(own))return;book.value=state.books.find(b=>b.id===result.book_id)||null;await startAccountDownload(account.value,result.book_id,'',true,3000,{directory:directory.value});if(valid(own))emit('saved',result.book_id);});}
async function saved(id:string){const own=epoch;try{const state=await snapshot();if(!valid(own)||book.value?.id!==id)return;book.value=state.books.find(b=>b.id===id)||null;emit('saved',id);}catch(e){if(valid(own))error.value=String(e);}}
onMounted(()=>void restore());onUnmounted(()=>{disposed=true;epoch++;});
</script>
<template>
  <section class="platform-save" aria-label="多平台账户连续保存">
    <h2>登录官网，连续保存 <small>实验性，真实已购待验收</small></h2>
    <p>1. 选择平台并登录　2. 读取并核对目录　3. 保存可读章节，自动生成 TXT</p>
    <label>平台<select v-model="platform" :disabled="busy||!!book"><option v-for="(name,key) in names" :key="key" :value="key">{{ name }}</option></select></label>
    <label>官网作品链接<input v-model="url" type="url" :disabled="busy||!!book" placeholder="粘贴当前平台的作品详情页链接，不含登录参数"></label>
    <p>仅保存官网正常显示且通过身份、字数及完整性检查的普通正文。七猫或咪咕的 App 专属内容、飞卢图片正文等不会因登录自动变为可保存文字。</p>
    <button :disabled="busy" @click="login">{{ opened?'重新打开官网登录窗口':'打开官网，去登录' }}</button>
    <button v-if="!book" :disabled="busy||!opened||!url.trim()" @click="catalog">{{ busy?'正在处理…':'我已登录，读取作品目录' }}</button>
    <section v-if="preview?.catalog"><h3>{{ preview.catalog.title }}</h3><p>{{ preview.catalog.author||'未标注作者' }} · {{ preview.catalog.chapters.length }} 章</p><p>首章：{{ preview.catalog.chapters[0]?.title }}<br>末章：{{ preview.catalog.chapters.at(-1)?.title }}</p><label class="ack"><input v-model="ack" type="checkbox" :disabled="busy">我已核对目录，关闭官网自动订阅，只保存已有阅读权限的内容</label><SaveLocation v-model="directory" :disabled="busy" @busy="choosingDirectory=$event" @update:model-value="directoryTouched=true" /><button :disabled="busy||choosingDirectory||!ack" @click="start">是这本，开始保存为 TXT</button></section>
    <p v-if="error" role="alert">{{ error }}；已保存内容保留。</p>
    <template v-if="book"><AutoSavePanel :key="book.id+account" :account-id="account" :opened="opened" :preferred-book="book.id" :revision="0" :session-source="source" :initial-directory="directoryTouched?directory:undefined" guided @saved="saved" @read="emit('read',$event)" /></template>
  </section>
</template>
<style scoped>
.platform-save{padding:24px;background:var(--surface);border:1px solid var(--border);border-radius:10px;margin:16px 0}.platform-save p{line-height:1.8}.platform-save label{display:grid;gap:8px;margin:16px 0}.platform-save input,.platform-save select{padding:10px;font:inherit;min-width:0}.platform-save button{margin:8px 12px 8px 0}.platform-save small{font-size:12px;color:var(--muted)}.platform-save .ack{display:flex;align-items:start}
</style>
