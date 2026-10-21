<script setup lang="ts">
import {ref} from 'vue';
import {backupLibrary,restoreLibrary,openRestoredLibrary,previewDiagnostics,saveDiagnostics,desktopAvailable,type MaintenanceReceipt,type DiagnosticReport} from './api';
const working=ref(false);const error=ref('');const notice=ref('');
const backup=ref<MaintenanceReceipt|null>(null);const restored=ref<MaintenanceReceipt|null>(null);const source=ref('');const diagnostic=ref<DiagnosticReport|null>(null);
async function action(work:()=>Promise<void>) {
  if(working.value)return;working.value=true;error.value='';notice.value='';
  try{await work();}catch(reason){error.value=String(reason);}finally{working.value=false;}
}
function createBackup(){void action(async()=>{backup.value=await backupLibrary();source.value=backup.value.path;notice.value='一致性备份完成，原书库继续可用。';});}
function restore(){void action(async()=>{restored.value=await restoreLibrary(source.value.trim());notice.value='已恢复到独立副本，当前书库保持打开。';});}
function openRestored(){void action(async()=>{if(restored.value){await openRestoredLibrary(restored.value.id);notice.value='正在独立窗口打开恢复的书库。';}});}
function preview(){void action(async()=>{diagnostic.value=await previewDiagnostics();});}
function save(){void action(async()=>{notice.value=`已保存本次预览的诊断：${await saveDiagnostics()}`;});}
</script>
<template>
  <div v-if="error" class="message failure" role="alert">{{ error }}</div>
  <div v-if="notice" class="message success" role="status">{{ notice }}</div>
  <section class="download-panel" aria-label="书库备份">
    <h2>一致性备份</h2><p class="footnote">备份数据库、已保存正文、标签和任务，并生成校验清单。导出记录保留，导出文件需在恢复书库中重新生成；账户凭据不进入备份。</p>
    <button :disabled="!desktopAvailable||working" @click="createBackup">创建书库备份</button>
    <p v-if="backup" class="path-result">{{ backup.books }} 本 / {{ backup.chapters }} 章<br>{{ backup.path }}</p>
  </section>
  <section class="download-panel" aria-label="恢复书库副本">
    <h2>恢复到新副本</h2><p class="footnote">填写含 manifest.json 和 library.db 的备份目录。校验成功后可在独立窗口打开，不覆盖当前书库；失败的副本不会替换原数据。</p>
    <label for="restore-path">备份目录</label><div class="link-row"><input id="restore-path" v-model="source" placeholder="粘贴备份目录的完整路径" :disabled="working"><button :disabled="!desktopAvailable||working||!source.trim()" @click="restore">校验并恢复副本</button></div>
    <p v-if="restored" class="path-result">已恢复 {{ restored.books }} 本 / {{ restored.chapters }} 章<br>{{ restored.path }}</p><button v-if="restored" :disabled="working" @click="openRestored">打开恢复的书库</button>
  </section>
  <section class="download-panel" aria-label="诊断信息">
    <h2>诊断预览</h2><p class="footnote">仅包含版本、系统、schema、数量、任务状态及完整性检查。不包含作品名、正文、标签、文件路径、账户或错误原文。先预览，再决定是否保存和分享。</p>
    <button :disabled="!desktopAvailable||working" @click="preview">预览诊断信息</button>
    <pre v-if="diagnostic" aria-label="诊断预览内容">{{ JSON.stringify(diagnostic,null,2) }}</pre>
    <button v-if="diagnostic" :disabled="working" @click="save">保存本次诊断预览</button>
  </section>
</template>
<style scoped>
.path-result{overflow-wrap:anywhere;font-size:13px;line-height:1.8}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#f4f6ef;padding:16px;border-radius:6px;font-size:12px}h2{margin-bottom:12px}.link-row input{min-width:0}
</style>
