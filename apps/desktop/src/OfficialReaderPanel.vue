<script setup lang="ts">
import {computed,onUnmounted,ref,watch} from 'vue';
import {officialBookPlatform} from './workflow';
import {openOfficialReader,inspectOfficialReader,saveOfficialReader,chooseOfficialExportDirectory,revealOfficialReader,closeOfficialReader,type OfficialPreview,type OfficialExport} from './api';
const props=defineProps<{initialUrl?:string;initialDirectory?:string|null}>();
const platform=ref('ciweimao');const bookUrl=ref('');const busy=ref(false);const opened=ref(false);const error=ref('');const notice=ref('');const ack=ref(false);
const preview=ref<OfficialPreview|null>(null);const receipt=ref<OfficialExport|null>(null);let disposed=false;let epoch=0;
const directory=ref(props.initialDirectory||'');
const names:Record<string,string>={ciweimao:'刺猬猫',faloo:'飞卢',migu:'咪咕悦看'};
const name=computed(()=>names[platform.value]||platform.value);
const statusText=computed(()=>({text:'已识别当前可见文字，尚未证明全章完整。',excerpt:'已识别咪咕官网公开试读。可保存试读 TXT；全文仍需在官网提示的 App 中阅读，登录不会自动解锁网页全文。',book:'已识别作品页。请在官网打开目标章节，再识别当前页。',restricted:'官网要求订阅或只提供试读。请在官网核对已有权益，再重新识别。',protected:'当前正文采用图片、画布或保护呈现，此入口无法保存普通文字。',verification:'官网要求验证，请本人在原窗口完成后重新识别。',unsupported:'当前页面结构尚未识别，可能仍在登录页或需要使用 App。',loading:'官网页面还在加载，请稍候重新识别。'}[preview.value?.page.status||'unsupported']));
watch(()=>props.initialUrl,value=>{if(value){const p=officialBookPlatform(value);if(p){platform.value=p;bookUrl.value=value;}}},{immediate:true});
watch([platform,bookUrl],()=>{epoch++;preview.value=null;ack.value=false;error.value='';notice.value='';opened.value=false;receipt.value=null;});
async function action(work:()=>Promise<void>){if(busy.value)return;const own=epoch;busy.value=true;error.value='';notice.value='';try{await work();}catch(e){if(!disposed&&own===epoch)error.value=String(e);}finally{if(!disposed)busy.value=false;}}
function open(login:boolean){void action(async()=>{const own=epoch;const p=platform.value;const input=bookUrl.value;if(!login&&officialBookPlatform(input)!==p)throw new Error('请填写所选平台的干净作品链接。');await openOfficialReader(p,input,login);if(!disposed&&own===epoch&&p===platform.value){opened.value=true;preview.value=null;notice.value=login?'请在官网自行登录，再打开你能正常阅读的目标章节。窗口打开不代表已经登录。':p==='migu'?'已打开官网手机试读页，等待加载完成后点击“识别官网试读”。无需先登录。':'作品页已打开，请在官网打开一章正文后回到这里识别。';}});}
function inspect(){void action(async()=>{const own=epoch;const p=platform.value;preview.value=null;ack.value=false;const result=await inspectOfficialReader(p);if(!disposed&&own===epoch&&p===platform.value)preview.value=result;});}
function save(){void action(async()=>{if(!preview.value||!ack.value)throw new Error('请重新识别并确认保存范围。');const own=epoch;const p=platform.value;const result=await saveOfficialReader(p,preview.value.token,ack.value,directory.value||undefined);if(!disposed&&own===epoch&&p===platform.value){receipt.value=result;preview.value=null;ack.value=false;notice.value='TXT 已保存。点击下面的按钮打开所在文件夹。';}});}
function chooseDirectory(){void action(async()=>{const own=epoch;const selected=await chooseOfficialExportDirectory();if(!disposed&&own===epoch&&selected)directory.value=selected;});}
function reveal(){const id=receipt.value?.id;if(id)void action(()=>revealOfficialReader(id));}
function close(){void action(async()=>{const own=epoch;const p=platform.value;await closeOfficialReader(p);if(!disposed&&own===epoch&&p===platform.value){opened.value=false;preview.value=null;notice.value='官网会话已清理，已经导出的 TXT 保留。';}});}
onUnmounted(()=>{disposed=true;epoch++;});
</script>
<template>
  <section class="official-reader" aria-label="其他平台登录与识别">
    <h2>登录官网，识别当前页 <small>实验性</small></h2>
    <p v-if="platform==='migu'">粘贴咪咕作品链接，打开这本书，再识别官网试读。公开试读无需登录，保存的 TXT 会标明试读范围并保留作品信息。</p>
    <p v-else>先在官网登录，再打开你能正常阅读的一章。此入口只采集当前页可见文字，不会自动保存全书。</p>
    <p>已购或会员权益内的章节也按官网实际可读正文识别，不另设会员下载配额。仅有 VIP 标志不代表所有作品均可阅读；各平台的全书自动保存需要独立适配。</p>
    <label>小说平台<select v-model="platform" :disabled="busy"><option value="ciweimao">刺猬猫</option><option value="faloo">飞卢</option><option value="migu">咪咕悦看</option></select></label>
    <label>作品链接<input v-model="bookUrl" :disabled="busy" type="url" :placeholder="platform==='migu'?'https://www.cmread.com/yuekan/novel/作品ID.html':'复制官网作品详情页地址'" /></label>
    <div class="actions"><button :disabled="busy" @click="open(true)">打开{{ name }}，去登录</button><button :disabled="busy||!bookUrl.trim()" @click="open(false)">打开这本书</button><button class="primary" :disabled="busy||!opened" @click="inspect">{{ busy?'正在处理…':platform==='migu'?'识别官网试读':'我已登录，识别当前页' }}</button></div>
    <p>登录和订阅操作由你本人在官网完成。请关闭自动订阅；软件不会替你购买、判断登录成功或自动通过验证码。</p>
    <section v-if="preview" aria-label="官网页面识别结果">
      <h3>{{ preview.page.book_title||name }} <small>{{ preview.page.chapter_title }}</small></h3><p role="status">{{ statusText }}</p>
      <div v-if="preview.page.status==='text'||preview.page.status==='excerpt'"><button :disabled="busy" @click="chooseDirectory">选择 TXT 保存文件夹</button><label>本次保存位置<input v-model="directory" :disabled="busy" placeholder="留空使用默认导出文件夹" /></label></div>
      <template v-if="preview.page.status==='text'||preview.page.status==='excerpt'"><p>识别到 {{ preview.page.paragraphs.length }} 段文字{{ preview.page.declared_words?'，官网声明 '+preview.page.declared_words+' 字':'，官网未提供可核验的字数' }}。TXT 将明确标注“{{ preview.page.status==='excerpt'?'官网公开试读':'当前页可见文字' }}”，不会标成完整章节。</p><p v-if="preview.page.author">作者：{{ preview.page.author }}</p><blockquote>{{ preview.page.paragraphs[0]?.slice(0,160) }}</blockquote><label class="ack"><input v-model="ack" type="checkbox" :disabled="busy">我有该内容的阅读权限，确认当前识别内容属于此作品，并接受当前页采集范围</label><button class="primary" :disabled="busy||!ack" @click="save">确认，保存当前页为 TXT</button></template>
    </section>
    <p v-if="error" role="alert">{{ error }}</p><p v-if="notice" role="status">{{ notice }}</p>
    <section v-if="receipt">
      <p>已保存：{{ receipt.path }}</p><button :disabled="busy" @click="reveal">打开 TXT 所在文件夹</button>
    </section>
    <details><summary>关闭并清理官网会话</summary><button :disabled="busy" @click="close">关闭{{ name }}会话</button><p>重新打开软件后需要重新登录。已导出的 TXT 不受影响。</p></details>
  </section>
</template>
<style scoped>
.official-reader{padding:24px;background:var(--surface);border:1px solid var(--border);border-radius:10px;margin:16px 0}.official-reader h2{font-size:22px}.official-reader p{line-height:1.9;font-size:14px;overflow-wrap:anywhere}.official-reader small{font-size:13px;color:var(--muted)}.official-reader label{display:grid;gap:8px;margin:14px 0}.official-reader input,.official-reader select{font:inherit;padding:10px;border:1px solid var(--border);border-radius:5px;min-width:0}.official-reader .ack{display:flex;align-items:start}.actions{display:flex;gap:12px;flex-wrap:wrap;margin:18px 0}blockquote{padding:12px;background:var(--accent-soft);margin:16px 0;line-height:1.8}details{margin-top:22px}
</style>
