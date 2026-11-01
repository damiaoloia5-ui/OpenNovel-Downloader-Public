<script setup lang="ts">
import {onMounted,onUnmounted,ref,watch} from 'vue';
import {desktopAvailable,localEngineStatus,setLocalEngine,type LocalEngineStatus} from './api';
const props=defineProps<{disabled?:boolean;current?:LocalEngineStatus|null}>();
const emit=defineEmits<{changed:[];checked:[status:LocalEngineStatus]}>();
const status=ref<LocalEngineStatus|null>(props.current??null);const busy=ref(false);const operation=ref<'check'|'prepare'|'disable'>('check');const error=ref('');let disposed=false;
async function check(){if(busy.value||props.disabled)return;busy.value=true;operation.value='check';error.value='';try{const value=await localEngineStatus();if(!disposed){status.value=value;emit('checked',value);emit('changed');}}catch(e){if(!disposed)error.value=String(e);}finally{busy.value=false;}}
async function setEnabled(enabled:boolean){if(busy.value||!status.value||props.disabled)return;busy.value=true;operation.value=enabled?'prepare':'disable';error.value='';try{const value=await setLocalEngine(enabled);if(!disposed){status.value=value;emit('checked',value);emit('changed');}}catch(e){if(!disposed)error.value=String(e);}finally{busy.value=false;}}
watch(()=>props.current,value=>{if(value)status.value=value;});
onMounted(()=>{if(desktopAvailable&&props.current===undefined)void check();});onUnmounted(()=>{disposed=true;});
</script>
<template>
  <details class="engine-options">
    <summary>{{ status?.enabled?'番茄本地增强已启用（已记住）':status?.remembered?'番茄本地增强需要恢复':'番茄保存增强与环境检查' }}</summary>
    <p>可尝试官方正常接口中明确免费的 App 章节；不接陌生公网解析，不发送账户凭据。需要额外的本地隔离组件，仍可能遇到访问限制。</p>
    <p role="status" :aria-busy="busy">{{ busy?(operation==='prepare'?'正在校验并准备内置引擎，首次可能需要几分钟，无需重复点击…':operation==='disable'?'正在关闭并清除启用记忆…':'正在检查组件，请稍候…'):status?.message||(desktopAvailable?'正在检查本地增强组件…':'请运行桌面应用查看组件状态。') }}</p>
    <p v-if="!busy&&status?.next_step" class="engine-next-step"><strong>下一步：</strong>{{ status.next_step }}</p>
    <div class="actions"><button type="button" :disabled="!desktopAvailable||busy||disabled" @click="check">重新检查组件</button><button v-if="!status?.enabled&&(status?.ready||status?.installed)" type="button" :disabled="!desktopAvailable||busy||disabled" @click="setEnabled(true)">{{ status?.remembered?'重新准备本地增强':'启用并记住选择' }}</button><button v-if="status?.enabled||status?.remembered" type="button" :disabled="!desktopAvailable||busy||disabled" @click="setEnabled(false)">关闭并忘记选择</button></div>
    <p v-if="status?.enabled" class="engine-enabled">已启用并记住：在上方粘贴番茄作品链接，点“开始保存为 TXT”。下次打开无需重复启用；可随时在这里关闭。</p>
    <p>引擎结果作为独立来源入库，与已有网页版本分开保存。遇到付费、验证或正文校验失败会停止，不自动购买或处理滑块。已有一本免费作品全目录 102 章实测通过；多作品、全站和独立安装环境仍待验收。</p>
    <details><summary>组件未就绪怎么办？</summary><p>一体化版已附带 Python、离线引擎和对应源码，无需下载镜像、配置环境变量或自行构建。点“启用并记住选择”时会校验并准备内置组件，必要时尝试启动已安装的 Docker Desktop。首次准备可能需要几分钟；失败后检查 Docker 和可用空间再重试，原进度保留。Docker 本身仍需安装。</p></details>
    <p v-if="error" class="message failure" role="alert">{{ error }}</p>
  </details>
</template>
<style scoped>
.engine-options{border-top:1px solid #d4ddca;padding-top:16px;margin-top:18px}.engine-options p{font-size:13px;line-height:1.8;color:#526048;max-width:75ch}.engine-options summary{cursor:pointer;font-size:13px;line-height:1.8}.actions{display:flex;gap:10px;flex-wrap:wrap;margin:12px 0}.engine-enabled{font-weight:600}
</style>
