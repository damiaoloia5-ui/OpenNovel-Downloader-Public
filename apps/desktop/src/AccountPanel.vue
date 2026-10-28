<script setup lang="ts">
import {computed,onMounted,onUnmounted,ref,watch} from 'vue';
import CapturePanel from './CapturePanel.vue';
import AutoSavePanel from './AutoSavePanel.vue';
import {accountList,createAccount,openAccount,closeAccount,previewAccountPage,confirmAccountPage,snapshot,type AccountStatus,type Book,type CapturePreview} from './api';
import {qidianBookId,qidianBookLink} from './workflow';
const props=defineProps<{bookId?:string;initialUrl?:string;accountId?:string;embedded?:boolean}>();
const emit=defineEmits<{saved:[bookId:string];read:[bookId:string];ready:[bookId:string,accountId:string];selected:[accountId:string]}>();
const accounts=ref<AccountStatus[]>([]);const chosen=ref('');const books=ref<Book[]>([]);const bookUrl=ref('');
const busy=ref(false);const error=ref('');const notice=ref('');const preview=ref<CapturePreview|null>(null);const previewAccount=ref('');const previewUrl=ref('');const catalogRevision=ref(0);
const platform=ref('qidian');const label=ref('');let disposed=false;
const qidianAccounts=computed(()=>accounts.value.filter(r=>r.account.platform==='qidian'));
const existingBooks=computed(()=>books.value.filter(b=>b.source==='qidian-session'));
const current=computed(()=>qidianAccounts.value.find(r=>r.account.id===chosen.value));
const opened=computed(()=>current.value?.session==='open_unverified');
const selectedBook=computed(()=>books.value.find(b=>b.source==='qidian-session'&&b.remote_id===qidianBookId(bookUrl.value)));
const step=computed(()=>!opened.value?1:!selectedBook.value?2:3);
async function reload(){accounts.value=await accountList();if(!qidianAccounts.value.some(r=>r.account.id===chosen.value))chosen.value=qidianAccounts.value.find(r=>r.account.id===props.accountId)?.account.id||qidianAccounts.value[0]?.account.id||'';books.value=(await snapshot()).books;}
async function action(work:()=>Promise<void>){if(busy.value)return;busy.value=true;error.value='';notice.value='';try{await work();await reload();}catch(e){error.value=String(e);}finally{busy.value=false;}}
function applyInput(){if(props.initialUrl){bookUrl.value=props.initialUrl;return;}const b=books.value.find(b=>b.id===props.bookId&&b.source==='qidian-session');if(b)bookUrl.value=`https://www.qidian.com/book/${b.remote_id}/`;}
function selectBook(event:Event){const id=(event.target as HTMLSelectElement).value;const b=existingBooks.value.find(b=>b.id===id);bookUrl.value=b?`https://www.qidian.com/book/${b.remote_id}/`:'';}
watch(()=>[props.bookId,props.initialUrl],applyInput);
watch([bookUrl,chosen],()=>{preview.value=null;error.value='';notice.value='';});
watch(chosen,id=>{if(id)emit('selected',id);});
async function ensureAccount(){if(chosen.value)return chosen.value;const a=await createAccount('qidian','我的起点账户');chosen.value=a.id;return a.id;}
function login(){void action(async()=>{const id=await ensureAccount();await openAccount(id,'',true);notice.value='官网已打开。请在官网亲自登录，再回到这里进行第 2 步；软件不会自动判断登录成功。';});}
function openBook(){void action(async()=>{const link=qidianBookLink(bookUrl.value);if(!link)throw new Error('请粘贴起点官网作品链接，例如 https://www.qidian.com/book/123/，不要包含登录参数。');const id=await ensureAccount();await openAccount(id,`${link}catalog/`);notice.value='作品目录已在官网打开。等待全部章节加载或展开后，点击“读取作品目录”。';});}
function readCatalog(){void action(async()=>{preview.value=null;const link=qidianBookLink(bookUrl.value);if(!link)throw new Error('请先填写起点作品链接并打开作品目录。');const id=chosen.value;const result=await previewAccountPage(id,'catalog');if(id!==chosen.value||link!==qidianBookLink(bookUrl.value))return;if(result.catalog?.remote_id!==qidianBookId(link))throw new Error('官网目录与填写的作品不一致，请点击“打开作品目录”后重试。');preview.value=result;previewAccount.value=id;previewUrl.value=link;});}
function confirmCatalog(){void action(async()=>{const value=preview.value;if(!value||previewAccount.value!==chosen.value||previewUrl.value!==qidianBookLink(bookUrl.value))throw new Error('账户或作品已改变，请重新读取目录。');preview.value=null;const result=await confirmAccountPage(chosen.value,value.token);if(disposed)return;catalogRevision.value++;notice.value='目录已确认，接着保存本书正文。';emit('saved',result.book_id);emit('ready',result.book_id,chosen.value);});}
function savedBook(id:string){void action(async()=>{await reload();if(props.embedded&&id!==props.bookId){notice.value='已保存另一部作品，请从书库打开它的工作台。';emit('saved',id);return;}const b=books.value.find(b=>b.id===id&&b.source==='qidian-session');if(b)bookUrl.value=`https://www.qidian.com/book/${b.remote_id}/`;catalogRevision.value++;emit('saved',id);});}
function add(){void action(async()=>{const a=await createAccount(platform.value,label.value);if(a.platform==='qidian')chosen.value=a.id;label.value='';notice.value='本地账户记录已添加。';});}
function close(id:string,remove:boolean){void action(async()=>{await closeAccount(id,remove);preview.value=null;notice.value=remove?'账户记录已移除，书库保留。':'会话已关闭，书库和进度保留。';});}
onMounted(()=>{void action(async()=>{await reload();applyInput();});});
onUnmounted(()=>{disposed=true;});
</script>
<template>
  <section class="account-panel" aria-label="起点保存引导">
    <h2>{{ embedded?'本书官网会话':'起点保存引导' }} <small>实验性</small></h2>
    <p>{{ embedded?'保持这本书的官网窗口打开，保存进度会同步到工作台。导出请切换到“导出文件”页签。':'按顺序操作，确认目录后进入本书工作台保存、阅读和导出。' }}</p>
    <ol v-if="!embedded" class="flow-steps" aria-label="起点保存步骤"><li :aria-current="step===1?'step':undefined"><b>1</b>打开官网登录<small>{{ opened?'窗口已打开':'待打开' }}</small></li><li :aria-current="step===2?'step':undefined"><b>2</b>确认作品目录<small>{{ selectedBook?'已有目录':'待确认' }}</small></li><li><b>3</b>进入作品工作台<small>保存、阅读与导出</small></li></ol>
    <label v-if="qidianAccounts.length>1">使用的起点账户<select v-model="chosen" :disabled="busy"><option v-for="row in qidianAccounts" :key="row.account.id" :value="row.account.id">{{ row.account.label }}</option></select></label>
    <section class="flow-section"><h3>1. 打开官网登录</h3><p>登录只在官网完成。{{ opened?'窗口已打开，如果还没登录，请在官网完成。':'首次点击会自动建立本地记录，无需填写别名。' }} 关闭官网窗口后需重新登录。</p><button :class="!opened?'primary':''" :disabled="busy" @click="login">{{ opened?'重新打开登录入口':'打开官网登录' }}</button><p v-if="current?.blocked_navigation" class="message failure">官网尝试了暂不支持的跳转。请先暂停自动任务，再在高级操作关闭会话并重新打开。</p></section>
    <section class="flow-section">
<h3>2. 确认作品目录</h3>
      <label v-if="existingBooks.length&&!embedded">继续已有作品<select :value="selectedBook?.id||''" :disabled="busy" @change="selectBook"><option value="">添加新作品，粘贴下方链接</option><option v-for="b in existingBooks" :key="b.id" :value="b.id">{{ b.title }}</option></select></label>
      <label v-if="!embedded">起点官网作品链接<input v-model="bookUrl" type="url" :disabled="busy" placeholder="https://www.qidian.com/book/作品ID/" /></label>
      <div class="actions"><button :class="opened&&!selectedBook?'primary':''" :disabled="busy||!bookUrl.trim()" @click="openBook">打开作品目录</button><button :disabled="busy||!opened||!qidianBookLink(bookUrl)" @click="readCatalog">{{ busy?'处理中…':'读取作品目录' }}</button></div>
      <p v-if="selectedBook" class="catalog-ready" role="status">已识别《{{ selectedBook.title }}》的 {{ selectedBook.total_chapters }} 章目录，可直接进行第 3 步。</p><p v-else>在官网切到目录，展开全部章节后，回到软件读取。先核对书名和章数，再确认。</p>
      <p v-if="preview?.catalog" class="footnote">账户目录只确认作品和章序，无法据此判断登录或订阅权益。所有未保存章节仍需在官网核对，软件不会自动购买。</p><section v-if="preview?.catalog" class="catalog-confirm" aria-label="核对作品目录"><h4>{{ preview.catalog.title }}</h4><p>{{ preview.catalog.author||'未标注作者' }} · {{ preview.catalog.chapters.length }} 章</p><p>首章：{{ preview.catalog.chapters[0]?.title }}<br>末章：{{ preview.catalog.chapters[preview.catalog.chapters.length-1]?.title }}</p><div class="actions"><button class="primary" :disabled="busy" @click="confirmCatalog">确认这本书</button><button :disabled="busy" @click="preview=null">重新选择</button></div></section>
    </section>
    <p v-if="error" class="message failure" role="alert">{{ error }}</p><p v-if="notice" class="message success" role="status">{{ notice }}</p>
    <button v-if="selectedBook&&!embedded" class="primary" :disabled="busy" @click="emit('ready',selectedBook.id,chosen)">进入本书工作台</button>
    <AutoSavePanel v-if="current&&selectedBook&&embedded" :key="`${chosen}:${selectedBook.id}`" :account-id="chosen" :opened="!!opened" :preferred-book="selectedBook.id" :revision="catalogRevision" guided hide-export @saved="emit('saved',$event)" @read="emit('read',$event)" />
    <p v-else class="next-step">下一步：{{ !opened?'先点击“打开官网登录”。':!bookUrl?'粘贴作品链接，再打开作品目录。':'读取并确认作品目录，随后会显示保存和导出按钮。' }}</p>
    <details class="advanced">
<summary>高级操作：手动逐章保存、账户管理、书旗窗口</summary>
      <CapturePanel v-if="current" :key="chosen" :account-id="chosen" :opened="!!opened" @saved="savedBook" @read="emit('read',$event)" />
      <h3>账户管理</h3><form class="account-form" @submit.prevent="add"><label>平台<select v-model="platform" :disabled="busy"><option value="qidian">起点中文网</option><option value="shuqi">书旗小说</option></select></label><label>本地别名<input v-model="label" maxlength="40" required :disabled="busy" placeholder="例如 第二个起点账户" /></label><button :disabled="busy||!label.trim()">添加账户记录</button></form>
      <ul class="managed-accounts"><li v-for="row in accounts" :key="row.account.id"><strong>{{ row.account.label }} · {{ row.account.platform==='qidian'?'起点':'书旗' }}</strong><span>{{ row.session==='closed'?'会话已关闭':'窗口已打开，权限未验证' }}</span><div class="actions"><button v-if="row.account.platform==='shuqi'" :disabled="busy" @click="action(async()=>{await openAccount(row.account.id,'');notice='已打开书旗官网，正文保存尚不支持。';})">打开书旗官网</button><button :disabled="busy||row.session==='closed'" @click="close(row.account.id,false)">关闭官网会话</button><button :disabled="busy" @click="close(row.account.id,true)">移除本地账户记录</button></div></li></ul><button :disabled="busy" @click="action(reload)">刷新账户状态</button>
    </details>
  </section>
</template>
<style scoped>
.account-panel{background:#fffefa;border:1px solid #e1e5da;border-radius:10px;padding:26px}.account-panel h2{font-size:22px}.account-panel small{font-size:12px;color:#526048;font-weight:400}.account-panel p{font-size:13px;color:#526048;line-height:1.9;max-width:74ch}.flow-steps{list-style:none;padding:0;display:flex;flex-wrap:wrap;gap:12px;margin:24px 0}.flow-steps li{flex:1;min-width:120px;padding:12px;background:#edf0e7;border-radius:6px;font-size:14px}.flow-steps li[aria-current=step]{background:#dce4cd;color:#273c29;outline:1px solid #8a9d72}.flow-steps b{margin-right:8px}.flow-steps small{display:block;margin:8px 0 0}.flow-section{padding:20px 0;border-top:1px solid #dfe4d7}.flow-section h3{font-size:18px;margin:0}.account-panel label{display:grid;gap:8px;font-size:13px;margin-top:14px}.account-panel input,.account-panel select{width:100%;min-width:0;border:1px solid #bdc9b1;background:#fafbf7;border-radius:5px;padding:11px;color:#334431;font:inherit}.account-panel input::placeholder{color:#617056}.actions,.account-form{display:flex;flex-wrap:wrap;gap:12px;margin:16px 0}.account-form{align-items:end}.account-form label{flex:1;min-width:150px}.catalog-ready{font-weight:600}.catalog-confirm{padding:16px;background:#edf1e5;border-radius:6px}.catalog-confirm h4{margin:0}.next-step{padding:16px;background:#edf1e5;border-radius:6px}.advanced{border-top:1px solid #dfe4d7;padding-top:20px;margin-top:26px}.advanced>summary{cursor:pointer;font-size:13px;line-height:1.8;color:#526048}.managed-accounts{list-style:none;padding:0}.managed-accounts li{padding:14px 0;border-bottom:1px solid #dfe4d7}.managed-accounts span{display:block;font-size:12px;margin-top:8px;color:#526048}
</style>
