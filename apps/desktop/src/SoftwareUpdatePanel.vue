<script setup lang="ts">
import {computed,onMounted,onUnmounted,ref} from 'vue';
import {version} from '../package.json';
import {desktopAvailable,softwareUpdateStatus,checkSoftwareUpdate,downloadSoftwareUpdate,installSoftwareUpdate,cancelSoftwareDownload,openSoftwareReleases,type SoftwareUpdateStatus} from './api';
const props=defineProps<{busy?:boolean}>();
const status=ref<SoftwareUpdateStatus|null>(null);
const error=ref('');
const working=ref(false);
const expanded=ref(false);
const convertPortable=ref(false);
const autoCheck=ref(true);
let stopped=false;
let pollTimer:ReturnType<typeof setTimeout>|undefined;
let startTimer:ReturnType<typeof setTimeout>|undefined;
const key='opennovel-software-auto-update-check';
const progress=computed(()=>status.value?.total?Math.min(100,Math.floor(status.value.downloaded/status.value.total*100)):null);
const installing=computed(()=>status.value?.phase==='installing');
const available=computed(()=>!!status.value?.version);
const mb=(bytes:number)=>(bytes/1024/1024).toFixed(1);
function refreshProgress(){if(pollTimer)clearTimeout(pollTimer);void poll();}
async function poll(){if(stopped||!desktopAvailable)return;try{const next=await softwareUpdateStatus();if(!stopped)status.value=next;}catch{/* A running installer closes the old IPC connection. */}finally{if(!stopped)pollTimer=setTimeout(()=>void poll(),working.value?400:5000);}}
function savePreference(){try{localStorage.setItem(key,String(autoCheck.value));}catch{error.value='无法保存自动检查偏好，下次启动仍会检查更新。';}}
async function check(automatic=false){if(working.value||!desktopAvailable)return;working.value=true;error.value='';try{status.value=await checkSoftwareUpdate();if(available.value)expanded.value=true;}catch(e){if(!automatic)error.value=String(e);}finally{working.value=false;}}
async function update(){if(working.value||props.busy)return;working.value=true;error.value='';expanded.value=true;refreshProgress();try{
  if(status.value?.phase!=='ready')status.value=await downloadSoftwareUpdate();
  if(stopped)return;
  // A verified download may stay ready while the user pauses tasks or closes a reader.
  await installSoftwareUpdate(convertPortable.value);
}catch(e){error.value=String(e);try{status.value=await softwareUpdateStatus();}catch{/* App may be exiting. */}}finally{working.value=false;}}
async function openReleases(){try{await openSoftwareReleases();}catch(e){error.value=String(e);}}
async function cancel(){try{await cancelSoftwareDownload();}catch(e){error.value=String(e);}}
onMounted(()=>{try{autoCheck.value=localStorage.getItem(key)!=='false';}catch{/* Default to a metadata-only check. */}void poll();if(autoCheck.value&&desktopAvailable)startTimer=setTimeout(()=>void check(true),2500);});
onUnmounted(()=>{stopped=true;if(pollTimer)clearTimeout(pollTimer);if(startTimer)clearTimeout(startTimer);});
</script>

<template>
  <details class="software-update" :open="expanded" @toggle="expanded=($event.target as HTMLDetailsElement).open">
    <summary>软件更新 <span>{{ available ? `发现 ${status?.version}` : `v${version}` }}</span></summary>
    <p role="status" aria-live="polite">{{ status?.message || '自动检查公开 GitHub 仓库的新版本，不上传书库或账号信息。' }}</p>
    <label class="update-preference"><input v-model="autoCheck" type="checkbox" :disabled="working" @change="savePreference" />启动时检查新版</label>
    <p v-if="error" class="message failure" role="alert">{{ error }}</p>
    <template v-if="status?.phase==='downloading'">
      <progress v-if="progress!==null" :value="progress" max="100" :aria-label="`更新下载 ${progress}%`"></progress>
      <progress v-else aria-label="正在下载安装包"></progress>
      <p>{{ mb(status.downloaded) }} MB<span v-if="status.total"> / {{ mb(status.total) }} MB</span></p>
      <button @click="cancel">取消下载</button>
    </template>
    <p v-if="status?.notes" class="update-notes">{{ status.notes }}</p>
    <label v-if="available&&status?.portable" class="update-preference"><input v-model="convertPortable" type="checkbox" :disabled="working" />此处运行的是便携版；同意转为安装版，保留原便携目录及书库。</label>
    <button v-if="available" class="primary" :disabled="working||busy||(status?.portable&&!convertPortable)" @click="update">{{ working?'正在处理…':status?.phase==='ready'?'备份书库并安装':'下载并安装更新' }}</button>
    <button :disabled="!desktopAvailable||working" @click="check(false)">{{ working&&status?.phase==='checking'?'正在检查…':'检查更新' }}</button>
    <button :disabled="!desktopAvailable||working" @click="openReleases">查看下载与历史版本</button>
    <p class="footnote">安装前请暂停保存任务、关闭官网阅读窗口。软件会先备份各用户书库；备份失败不会安装。已有导出文件和账号记录保留，登录有效期由平台决定。</p>
    <p v-if="status?.backup_directory" class="update-backup">本次备份：{{ status.backup_directory }}</p>
  </details>
  <div v-if="installing" class="update-install-overlay" role="alert" aria-live="assertive"><div><h2>正在安全更新</h2><p>{{ status?.message }}</p><p>请等待安装完成，暂时不要关闭软件或开始新的保存任务。</p></div></div>
</template>

<style scoped>
.software-update{border:1px solid var(--border,#d8e3df);border-radius:12px;padding:12px 16px;margin-bottom:20px;background:var(--panel,#fff)}
.software-update summary{cursor:pointer;font-weight:700}.software-update summary span{font-weight:400;margin-left:12px;font-size:14px;color:var(--muted,#65776d)}
.software-update p{line-height:1.7}.software-update button{margin:8px 10px 8px 0}.software-update progress{width:100%;height:10px;accent-color:#237759}
.update-preference{display:flex;align-items:center;gap:8px;margin:12px 0;font-size:14px}.update-preference input{width:auto;min-height:initial}
.update-notes{white-space:pre-wrap;max-height:200px;overflow:auto}.update-backup{overflow-wrap:anywhere;font-size:13px}
.update-install-overlay{position:fixed;inset:0;z-index:10000;background:#102b23b3;display:grid;place-items:center;padding:24px}.update-install-overlay>div{background:white;color:#17362b;border-radius:16px;padding:32px;max-width:560px}
</style>
