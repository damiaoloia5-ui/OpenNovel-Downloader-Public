<script setup lang="ts">
import {onMounted,onUnmounted,ref} from 'vue';
import {desktopAvailable,localEngineStatus,setLocalEngine,type LocalEngineStatus} from './api';
const props=defineProps<{disabled?:boolean}>();
const emit=defineEmits<{changed:[]}>();
const status=ref<LocalEngineStatus|null>(null);const busy=ref(false);const error=ref('');let disposed=false;
async function check(){if(busy.value)return;busy.value=true;error.value='';try{const value=await localEngineStatus();if(!disposed){status.value=value;emit('changed');}}catch(e){if(!disposed)error.value=String(e);}finally{busy.value=false;}}
async function toggle(){if(busy.value||!status.value||props.disabled)return;busy.value=true;error.value='';try{const value=await setLocalEngine(!status.value.enabled);if(!disposed){status.value=value;emit('changed');}}catch(e){if(!disposed)error.value=String(e);}finally{busy.value=false;}}
onMounted(()=>{if(desktopAvailable)void check();});onUnmounted(()=>{disposed=true;});
</script>
<template>
  <details class="engine-options">
    <summary>番茄网页只保存了前几章？本地引擎实验（默认关闭）</summary>
    <p>可尝试官方正常接口中明确免费的 App 章节；不接陌生公网解析，不发送账户凭据。需要额外的本地隔离组件，仍可能遇到访问限制。</p>
    <p role="status">{{ busy?'正在检查，请稍候…':status?.message||'请运行桌面应用查看组件状态。' }}</p>
    <div class="actions"><button type="button" :disabled="!desktopAvailable||busy||disabled" @click="check">重新检查组件</button><button v-if="status?.ready" type="button" :aria-pressed="status.enabled" :disabled="busy||disabled" @click="toggle">{{ status.enabled?'关闭本地引擎':'启用本地引擎实验' }}</button></div>
    <p v-if="status?.enabled" class="engine-enabled">已启用：在上方粘贴番茄作品链接，点“开始保存为 TXT”。暂停后可在本书进度继续；重启软件后需重新启用。</p>
    <p>引擎结果作为独立来源入库，与已有网页版本分开保存。遇到付费、验证或正文校验失败会停止，不自动购买或处理滑块。尚未完成多作品、全书及独立安装环境验收。</p>
    <details><summary>组件未就绪怎么办？</summary><p>当前为开发预览接入，尚未随安装包分发引擎。维护者请按项目 docs/local-engine.md 准备固定构建、配置 Python 后启动新版程序；普通网页功能可以继续使用。</p></details>
    <p v-if="error" class="message failure" role="alert">{{ error }}</p>
  </details>
</template>
<style scoped>
.engine-options{border-top:1px solid #d4ddca;padding-top:16px;margin-top:18px}.engine-options p{font-size:13px;line-height:1.8;color:#526048;max-width:75ch}.engine-options summary{cursor:pointer;font-size:13px;line-height:1.8}.actions{display:flex;gap:10px;flex-wrap:wrap;margin:12px 0}.engine-enabled{font-weight:600}
</style>
