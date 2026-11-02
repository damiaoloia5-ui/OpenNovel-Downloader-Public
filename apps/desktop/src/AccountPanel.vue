<script setup lang="ts">
import {computed,onMounted,onUnmounted,ref,watch} from 'vue';
import CapturePanel from './CapturePanel.vue';
import AutoSavePanel from './AutoSavePanel.vue';
import OfficialReaderPanel from './OfficialReaderPanel.vue';
import {accountList,createAccount,openAccount,closeAccount,previewAccountPage,confirmAccountPage,startAccountDownload,snapshot,type AccountStatus,type Book,type CapturePreview} from './api';
import {qidianBookId,qidianBookLink,officialBookPlatform} from './workflow';
import {readGuidedCatalog} from './guidedCatalog';
const props=defineProps<{bookId?:string;initialUrl?:string;accountId?:string;embedded?:boolean}>();
const emit=defineEmits<{saved:[bookId:string];read:[bookId:string];ready:[bookId:string,accountId:string];selected:[accountId:string]}>();
const accounts=ref<AccountStatus[]>([]);const chosen=ref('');const books=ref<Book[]>([]);const bookUrl=ref('');
const busy=ref(false);const error=ref('');const notice=ref('');const preview=ref<CapturePreview|null>(null);const previewAccount=ref('');const previewUrl=ref('');const catalogRevision=ref(0);
const platform=ref('qidian');const label=ref('');const loginAcknowledged=ref(false);const acknowledged=ref(false);let disposed=false;let readEpoch=0;
const qidianAccounts=computed(()=>accounts.value.filter(r=>r.account.platform==='qidian'));
const existingBooks=computed(()=>books.value.filter(b=>b.source==='qidian-session'));
const current=computed(()=>qidianAccounts.value.find(r=>r.account.id===chosen.value));
const opened=computed(()=>current.value?.session==='open_unverified');
const selectedBook=computed(()=>books.value.find(b=>b.source==='qidian-session'&&b.remote_id===qidianBookId(bookUrl.value)));
const step=computed(()=>!opened.value?1:selectedBook.value&&!preview.value?3:loginAcknowledged.value?2:1);
async function reload(){accounts.value=await accountList();if(!qidianAccounts.value.some(r=>r.account.id===chosen.value))chosen.value=qidianAccounts.value.find(r=>r.account.id===props.accountId)?.account.id||qidianAccounts.value[0]?.account.id||'';books.value=(await snapshot()).books;}
async function action(work:()=>Promise<void>){if(busy.value)return;busy.value=true;error.value='';notice.value='';try{await work();}catch(e){if(!disposed)error.value=String(e);}finally{if(!disposed){try{await reload();}catch(e){error.value=String(e);}}busy.value=false;}}
function applyInput(){if(props.initialUrl){bookUrl.value=props.initialUrl;return;}const b=books.value.find(b=>b.id===props.bookId&&b.source==='qidian-session');if(b)bookUrl.value=`https://www.qidian.com/book/${b.remote_id}/`;}
function selectBook(event:Event){const id=(event.target as HTMLSelectElement).value;const b=existingBooks.value.find(b=>b.id===id);bookUrl.value=b?`https://www.qidian.com/book/${b.remote_id}/`:'';}
watch(()=>[props.bookId,props.initialUrl],applyInput);
watch([bookUrl,chosen],()=>{readEpoch++;preview.value=null;acknowledged.value=false;loginAcknowledged.value=false;error.value='';notice.value='';});
watch(chosen,id=>{if(id)emit('selected',id);});
async function ensureAccount(){if(chosen.value)return chosen.value;const a=await createAccount('qidian','我的起点账户');chosen.value=a.id;return a.id;}
function login(){void action(async()=>{const id=await ensureAccount();await openAccount(id,'',true);notice.value='请在刚打开的起点窗口完成登录，然后回到这里点“我已登录，下一步”。';});}
function openBook(){void action(async()=>{const link=qidianBookLink(bookUrl.value);if(!link)throw new Error('请粘贴起点官网作品链接，例如 https://www.qidian.com/book/123/，不要包含登录参数。');const id=await ensureAccount();await openAccount(id,`${link}catalog/`);notice.value='作品目录已在官网打开。等待全部章节加载或展开后，点击“读取作品目录”。';});}
function readCatalog(){void action(async()=>{preview.value=null;acknowledged.value=false;const epoch=++readEpoch;const link=qidianBookLink(bookUrl.value);if(!link)throw new Error('请先填写起点作品链接并打开作品目录。');const id=chosen.value;const result=await previewAccountPage(id,'catalog');if(disposed||readEpoch!==epoch||id!==chosen.value||link!==qidianBookLink(bookUrl.value))return;if(result.catalog?.remote_id!==qidianBookId(link))throw new Error('官网目录与填写的作品不一致，请点击“打开作品目录”后重试。');preview.value=result;previewAccount.value=id;previewUrl.value=link;});}
function collectCatalog(navigate=true){void action(async()=>{const link=qidianBookLink(bookUrl.value);if(!link)throw new Error('请先填写这本书的起点官网链接。');const id=chosen.value;const epoch=++readEpoch;const current=()=>!disposed&&readEpoch===epoch&&chosen.value===id&&qidianBookLink(bookUrl.value)===link;preview.value=null;acknowledged.value=false;if(navigate)await openAccount(id,`${link}catalog/`);if(!current())return;const result=await readGuidedCatalog(link,()=>previewAccountPage(id,'catalog'),current,()=>new Promise(resolve=>setTimeout(resolve,1200)));if(result){preview.value=result;previewAccount.value=id;previewUrl.value=link;}});}
function loggedIn(){loginAcknowledged.value=true;if(selectedBook.value){emit('ready',selectedBook.value.id,chosen.value);return;}collectCatalog();}
function confirmCatalog(){void action(async()=>{const value=preview.value;const id=chosen.value;const link=qidianBookLink(bookUrl.value);const epoch=readEpoch;if(!acknowledged.value)throw new Error('请先确认已关闭官网自动订阅，并拥有要保存内容的阅读权限。');if(!value||previewAccount.value!==id||previewUrl.value!==link)throw new Error('账户或作品已改变，请重新读取目录。');const result=await confirmAccountPage(id,value.token);if(disposed||readEpoch!==epoch||chosen.value!==id||qidianBookLink(bookUrl.value)!==link)return;preview.value=null;catalogRevision.value++;emit('saved',result.book_id);await startAccountDownload(id,result.book_id,'',true);if(disposed||readEpoch!==epoch)return;notice.value='已开始保存，结束后会自动生成 TXT。';emit('ready',result.book_id,id);});}
function savedBook(id:string){void action(async()=>{await reload();if(props.embedded&&id!==props.bookId){notice.value='已保存另一部作品，请从书库打开它的工作台。';emit('saved',id);return;}const b=books.value.find(b=>b.id===id&&b.source==='qidian-session');if(b)bookUrl.value=`https://www.qidian.com/book/${b.remote_id}/`;catalogRevision.value++;emit('saved',id);});}
function add(){void action(async()=>{const a=await createAccount(platform.value,label.value);if(a.platform==='qidian')chosen.value=a.id;label.value='';notice.value='本地账户记录已添加。';});}
function close(id:string,remove:boolean){void action(async()=>{await closeAccount(id,remove);preview.value=null;notice.value=remove?'账户记录已移除，书库保留。':'会话已关闭，书库和进度保留。';});}
function refocus(){if(!busy.value)void action(reload);}
onMounted(()=>{void action(async()=>{await reload();applyInput();});window.addEventListener('focus',refocus);});
onUnmounted(()=>{disposed=true;readEpoch++;window.removeEventListener('focus',refocus);});
</script>
<template>
  <OfficialReaderPanel v-if="!embedded&&officialBookPlatform(initialUrl||'')" :initial-url="initialUrl" />
  <details v-else-if="!embedded"><summary>刺猬猫、飞卢、咪咕：登录后识别当前页</summary><OfficialReaderPanel /></details>
  <section v-if="!officialBookPlatform(initialUrl||'')" class="account-panel" aria-label="起点保存引导">
    <h2>{{ embedded?'保存这本书':'跟着提示保存小说' }} <small>起点 · 实验性</small></h2>
    <p>可连续保存账号实际能完整阅读的章节，软件不按会员等级设置字数或每日下载配额。VIP 身份、包月和单章订阅的范围不同，以这本书在官网的实际阅读权益为准。</p>
    <ol v-if="!embedded" class="flow-steps" aria-label="起点保存步骤"><li :aria-current="step===1?'step':undefined"><b>1</b>登录起点</li><li :aria-current="step===2?'step':undefined"><b>2</b>核对这本书</li><li :aria-current="step===3?'step':undefined"><b>3</b>保存并得到 TXT</li></ol>
    <label v-if="!embedded&&!qidianBookLink(bookUrl)">这本书的起点官网链接<input v-model="bookUrl" type="url" :disabled="busy" placeholder="从起点复制作品链接，粘贴到这里" /></label>
    <label v-if="qidianAccounts.length>1">使用哪个起点账号？<select v-model="chosen" :disabled="busy"><option v-for="row in qidianAccounts" :key="row.account.id" :value="row.account.id">{{ row.account.label }}</option></select></label>
    <section v-if="!opened||(!selectedBook&&!loginAcknowledged)" class="flow-section" aria-label="登录起点">
      <h3>{{ opened?'请完成官网登录，再回到这里':'先登录你的起点账号' }}</h3>
      <p>{{ opened?'软件不会替你判断是否登录成功。确认官网能正常阅读后，点下面的“下一步”。':'点击下面的按钮，在打开的起点窗口扫码或输入账号登录。登录后回到这个窗口。' }}</p>
      <div class="actions"><button v-if="!opened" class="primary" :disabled="busy||!qidianBookLink(bookUrl)" @click="login">打开起点，去登录</button><template v-else><button class="primary" :disabled="busy||!qidianBookLink(bookUrl)" @click="loggedIn">我已登录，下一步</button><button :disabled="busy" @click="login">没看到登录窗口？重新打开</button></template></div>
      <p v-if="!bookUrl">先在上方粘贴小说链接，再打开登录。</p>
    </section>
    <section v-else-if="preview||!selectedBook" class="flow-section" aria-label="核对这本书">
      <template v-if="preview?.catalog">
<h3>是这本书吗？</h3><section class="catalog-confirm" aria-label="核对作品目录"><h4>{{ preview.catalog.title }}</h4><p>{{ preview.catalog.author||'未标注作者' }} · 目录共 {{ preview.catalog.chapters.length }} 章</p><p>首章：{{ preview.catalog.chapters[0]?.title }}<br>末章：{{ preview.catalog.chapters[preview.catalog.chapters.length-1]?.title }}</p></section>
        <p>请核对书名和章数。接下来会保存可正常阅读的正文，结束后自动生成 TXT；遇到限制会停止并保留已保存内容。</p>
        <label class="ack"><input v-model="acknowledged" type="checkbox" :disabled="busy">我已在官网关闭自动订阅，确认只保存已有阅读权限的内容</label>
        <div class="actions"><button class="primary" :disabled="busy||!acknowledged" @click="confirmCatalog">{{ busy?'正在开始保存…':'是这本，开始保存为 TXT' }}</button><button :disabled="busy" @click="preview=null;acknowledged=false">不是这本，重新读取</button></div>
        <p v-if="!acknowledged">先勾选上面的确认项，才能开始保存。目录章数不代表已订阅或已保存章数。</p>
      </template>
      <template v-else><h3>{{ busy?'正在识别这本书，请稍等…':'还没读到完整目录' }}</h3><p>{{ busy?'软件正在打开作品目录并尝试读取，不需要你来回点击。':'如果官网要求登录或验证，请先完成；如果目录没有全部显示，请在官网点“展开全部”，再点下面的按钮。' }}</p><div v-if="!busy" class="actions"><button class="primary" @click="collectCatalog(false)">我已处理，重新读取</button><button @click="collectCatalog()">重新打开这本书</button></div></template>
    </section>
    <AutoSavePanel v-else-if="current&&selectedBook" :key="`${chosen}:${selectedBook.id}`" :account-id="chosen" :opened="!!opened" :preferred-book="selectedBook.id" :revision="catalogRevision" guided hide-export @check-session="refocus" @saved="emit('saved',$event)" @read="emit('read',$event)" />
    <p v-if="current?.blocked_navigation" class="message failure">官网有一个跳转未能打开。可在下方“更多设置”中关闭官网窗口，再重新登录。</p>
    <p v-if="error" class="message failure" role="alert">{{ error }}<br>下一步：先核对官网窗口的提示，再重试当前步骤。已保存的正文会保留。</p><p v-if="notice" class="message success" role="status">{{ notice }}</p>
    <button v-if="loginAcknowledged&&!selectedBook&&!busy" class="back-login" @click="loginAcknowledged=false;preview=null;acknowledged=false">返回登录步骤</button>
    <details class="advanced">
<summary>更多设置：更换作品、手动读取、账号管理</summary>
      <label v-if="existingBooks.length&&!embedded">继续书库中的小说<select :value="selectedBook?.id||''" :disabled="busy" @change="selectBook"><option value="">添加新小说</option><option v-for="b in existingBooks" :key="b.id" :value="b.id">{{ b.title }}</option></select></label>
      <label v-if="!embedded">更换起点作品链接<input v-model="bookUrl" type="url" :disabled="busy" placeholder="https://www.qidian.com/book/作品ID/" /></label>
      <div class="actions"><button :disabled="busy||!bookUrl.trim()" @click="openBook">手动打开作品目录</button><button :disabled="busy||!opened||!qidianBookLink(bookUrl)" @click="readCatalog">手动读取当前目录</button></div>
      <CapturePanel v-if="current" :key="chosen" :account-id="chosen" :opened="!!opened" @saved="savedBook" @read="emit('read',$event)" />
      <h3>账户管理</h3><form class="account-form" @submit.prevent="add"><label>平台<select v-model="platform" :disabled="busy"><option value="qidian">起点中文网</option><option value="shuqi">书旗小说</option></select></label><label>本地别名<input v-model="label" maxlength="40" required :disabled="busy" placeholder="例如 第二个起点账户" /></label><button :disabled="busy||!label.trim()">添加账户记录</button></form>
      <ul class="managed-accounts"><li v-for="row in accounts" :key="row.account.id"><strong>{{ row.account.label }} · {{ row.account.platform==='qidian'?'起点':'书旗' }}</strong><span>{{ row.session==='closed'?'会话已关闭':'窗口已打开，权限未验证' }}</span><div class="actions"><button v-if="row.account.platform==='shuqi'" :disabled="busy" @click="action(async()=>{await openAccount(row.account.id,'');notice='已打开书旗官网，正文保存尚不支持。';})">打开书旗官网</button><button :disabled="busy||row.session==='closed'" @click="close(row.account.id,false)">关闭官网会话</button><button :disabled="busy" @click="close(row.account.id,true)">移除本地账户记录</button></div></li></ul><button :disabled="busy" @click="action(reload)">刷新账户状态</button>
    </details>
  </section>
</template>
<style scoped>
.account-panel{background:var(--surface);border:1px solid var(--border);border-radius:10px;padding:26px}.account-panel h2{font-size:22px}.account-panel small{font-size:12px;color:var(--muted);font-weight:400}.account-panel p{font-size:13px;color:var(--muted);line-height:1.9;max-width:74ch}.flow-steps{list-style:none;padding:0;display:flex;flex-wrap:wrap;gap:12px;margin:24px 0}.flow-steps li{flex:1;min-width:120px;padding:12px;background:var(--surface-soft);border-radius:6px;font-size:14px}.flow-steps li[aria-current=step]{background:var(--accent-soft);color:var(--accent-strong);outline:1px solid var(--accent)}.flow-steps b{margin-right:8px}.flow-steps small{display:block;margin:8px 0 0}.flow-section{padding:20px 0;border-top:1px solid var(--border)}.flow-section h3{font-size:18px;margin:0}.account-panel label{display:grid;gap:8px;font-size:13px;margin-top:14px}.account-panel input,.account-panel select{width:100%;min-width:0;border:1px solid var(--border);background:var(--surface-soft);border-radius:5px;padding:11px;color:var(--ink);font:inherit}.account-panel input::placeholder{color:var(--muted)}.account-panel .ack{display:flex;align-items:start;gap:8px;line-height:1.8}.ack input{width:auto;margin-top:5px}.back-login{margin:14px 0}.actions,.account-form{display:flex;flex-wrap:wrap;gap:12px;margin:16px 0}.account-form{align-items:end}.account-form label{flex:1;min-width:150px}.catalog-ready{font-weight:600}.catalog-confirm{padding:16px;background:var(--accent-soft);border-radius:6px}.catalog-confirm h4{margin:0}.next-step{padding:16px;background:var(--accent-soft);border-radius:6px}.advanced{border-top:1px solid var(--border);padding-top:20px;margin-top:26px}.advanced>summary{cursor:pointer;font-size:13px;line-height:1.8;color:var(--muted)}.managed-accounts{list-style:none;padding:0}.managed-accounts li{padding:14px 0;border-bottom:1px solid var(--border)}.managed-accounts span{display:block;font-size:12px;margin-top:8px;color:var(--muted)}
</style>
