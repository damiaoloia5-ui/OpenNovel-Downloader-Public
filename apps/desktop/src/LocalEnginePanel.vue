<script setup lang="ts">
import {onMounted,onUnmounted,ref,watch} from 'vue';
import {desktopAvailable,localEngineStatus,setLocalEngine,type LocalEngineStatus} from './api';
const props=defineProps<{disabled?:boolean;current?:LocalEngineStatus|null}>();
const emit=defineEmits<{changed:[];checked:[status:LocalEngineStatus]}>();
const status=ref<LocalEngineStatus|null>(props.current??null);const busy=ref(false);const error=ref('');let disposed=false;
async function check(){if(busy.value)return;busy.value=true;error.value='';try{const value=await localEngineStatus();if(!disposed){status.value=value;emit('checked',value);emit('changed');}}catch(e){if(!disposed)error.value=String(e);}finally{busy.value=false;}}
async function toggle(){if(busy.value||!status.value||props.disabled)return;busy.value=true;error.value='';try{const value=await setLocalEngine(!status.value.enabled);if(!disposed){status.value=value;emit('checked',value);emit('changed');}}catch(e){if(!disposed)error.value=String(e);}finally{busy.value=false;}}
watch(()=>props.current,value=>{if(value)status.value=value;});
onMounted(()=>{if(desktopAvailable&&props.current===undefined)void check();});onUnmounted(()=>{disposed=true;});
</script>
<template>
  <details class="engine-options">
    <summary>{{ status?.enabled?'番茄本地增强已启用（已记住）':'番茄保存增强与环境检查' }}</summary>
    <p>可尝试官方正常接口中明确免费的 App 章节；不接陌生公网解析，不发送账户凭据。需要额外的本地隔离组件，仍可能遇到访问限制。</p>
    <p role="status">{{ busy?'正在检查，请稍候…':status?.message||(desktopAvailable?'正在检查本地增强组件…':'请运行桌面应用查看组件状态。') }}</p>
    <div class="actions"><button type="button" :disabled="!desktopAvailable||busy||disabled" @click="check">重新检查组件</button><button v-if="status?.ready||status?.installed" type="button" :aria-pressed="status.enabled" :disabled="busy||disabled" @click="toggle">{{ status.enabled?'关闭本地增强':'启用并记住选择' }}</button></div>
    <p v-if="status?.enabled" class="engine-enabled">已启用并记住：在上方粘贴番茄作品链接，点“开始保存为 TXT”。下次打开无需重复启用；可随时在这里关闭。</p>
    <p>引擎结果作为独立来源入库，与已有网页版本分开保存。遇到付费、验证或正文校验失败会停止，不自动购买或处理滑块。已有一本免费作品全目录 102 章实测通过；多作品、全站和独立安装环境仍待验收。</p>
    <details><summary>组件未就绪怎么办？</summary><p>一体化版已附带 Python 和组件，无需设置环境变量或运行启动脚本。点“启用并记住选择”时会尝试启动已安装的 Docker Desktop；如果仍失败，请启动 Docker Desktop 后重新检查。镜像或固定构建缺失时仍需按组件说明准备；普通网页功能可以继续使用。</p></details>
    <p v-if="error" class="message failure" role="alert">{{ error }}</p>
  </details>
</template>
<style scoped>
.engine-options{border-top:1px solid #d4ddca;padding-top:16px;margin-top:18px}.engine-options p{font-size:13px;line-height:1.8;color:#526048;max-width:75ch}.engine-options summary{cursor:pointer;font-size:13px;line-height:1.8}.actions{display:flex;gap:10px;flex-wrap:wrap;margin:12px 0}.engine-enabled{font-weight:600}
</style>
