<script setup lang="ts">
import {computed,onMounted,ref} from 'vue';
import {createLocalProfile,desktopAvailable,localProfiles,switchLocalProfile,type ProfileOverview} from './api';
const state=ref<ProfileOverview|null>(null),selected=ref(''),name=ref(''),busy=ref(false),error=ref(''),notice=ref('');
const current=computed(()=>state.value?.profiles.find(p=>p.id===state.value?.current_id));
async function reload(){state.value=await localProfiles();selected.value=state.value.current_id;}
async function create(){if(busy.value)return;busy.value=true;error.value='';notice.value='';try{const p=await createLocalProfile(name.value.trim());await reload();selected.value=p.id;name.value='';notice.value='新用户已建立，书库为空。点击“切换用户”开始使用。';}catch(e){error.value=String(e);}finally{busy.value=false;}}
async function change(){if(busy.value||!selected.value||selected.value===state.value?.current_id)return;busy.value=true;error.value='';notice.value='';try{await switchLocalProfile(selected.value);notice.value='正在重新打开所选用户的数据空间…';}catch(e){error.value=String(e);busy.value=false;}}
onMounted(async()=>{if(!desktopAvailable)return;try{await reload();}catch(e){error.value=String(e);}});
</script>
<template>
  <section class="profile-panel" aria-label="本地用户">
    <strong>当前用户：{{ current?.name||'正在读取' }}</strong>
    <details>
<summary>切换／新建用户</summary>
      <p>每个用户独立保存书籍、任务、登录会话和导出文件。切换会重开程序，请先暂停保存；登录窗口会关闭。</p>
      <label>选择用户<select v-model="selected" :disabled="busy||!state"><option v-for="p in state?.profiles||[]" :key="p.id" :value="p.id">{{ p.name }}</option></select></label>
      <button :disabled="busy||!selected||selected===state?.current_id" @click="change">{{ busy?'正在处理…':'切换用户' }}</button>
      <form @submit.prevent="create"><label>新用户名称<input v-model="name" maxlength="40" :disabled="busy||!state" placeholder="例如：我的学习书库"></label><button :disabled="busy||!state||!name.trim()">新建空白用户</button></form>
      <p>这是本地数据分区，不是密码保护。共用 Windows 账号的人仍能切换；需要防止他人访问，请使用独立 Windows 账号。手动选择公共导出文件夹会共享该位置。</p>
      <details v-if="state"><summary>当前用户数据位置</summary><code>{{ state.data_directory }}</code></details>
    </details>
    <p v-if="error" role="alert">{{ error }}</p><p v-if="notice" role="status">{{ notice }}</p>
  </section>
</template>
<style scoped>
.profile-panel{color:var(--ink);margin:18px 0;padding:14px 12px;border:1px solid var(--border);border-radius:8px;font-size:12px;background:var(--surface-soft);overflow-wrap:anywhere}.profile-panel strong{display:block;font-size:13px;margin-bottom:10px}.profile-panel p{font-size:12px;line-height:1.7;color:var(--muted)}.profile-panel label{display:grid;gap:6px;margin:10px 0}.profile-panel select,.profile-panel input{min-width:0;box-sizing:border-box;width:100%;padding:8px;background:var(--surface);color:var(--ink);border:1px solid var(--border);border-radius:5px}.profile-panel button{margin:4px 0;padding:8px 10px;font-size:12px}.profile-panel form{border-top:1px solid var(--border);margin-top:12px}.profile-panel code{font-size:11px}.profile-panel p[role=alert]{color:var(--danger)}
@media(max-width:760px){.profile-panel{position:fixed;z-index:20;left:94px;right:16px;top:72px;margin:0;max-height:calc(100vh - 100px);overflow:auto;padding:10px 14px;box-shadow:0 4px 16px #163b2520}.profile-panel strong{margin-bottom:4px}.profile-panel:not(:has(>details[open])){position:static;padding:6px 3px;box-shadow:none}.profile-panel:not(:has(>details[open]))>strong{font-size:10px}.profile-panel:not(:has(>details[open]))>details>summary{font-size:10px}}
</style>
