<script setup lang="ts">
import {onMounted,ref} from 'vue';
import CapturePanel from './CapturePanel.vue';
import AutoSavePanel from './AutoSavePanel.vue';
const props=defineProps<{bookId?:string}>();
const selectedBook=ref('');const catalogRevision=ref(0);
function savedBook(id:string){selectedBook.value=id;catalogRevision.value++;emit('saved',id);}
const emit=defineEmits<{saved:[bookId:string];read:[bookId:string]}>();
import {accountList,createAccount,openAccount,closeAccount,type AccountStatus} from './api';
const accounts=ref<AccountStatus[]>([]);const platform=ref('qidian');const label=ref('');const bookUrl=ref('');const busy=ref(false);const error=ref('');const notice=ref('');
async function reload(){accounts.value=await accountList();}
async function action(work:()=>Promise<void>){if(busy.value)return;busy.value=true;error.value='';notice.value='';try{await work();await reload();}catch(e){error.value=String(e);}finally{busy.value=false;}}
function add(){void action(async()=>{await createAccount(platform.value,label.value);label.value='';notice.value='已建立本地记录。起点请点“登录起点”，其他平台使用官网窗口。';});}
function open(id:string,login=false){void action(async()=>{await openAccount(id,bookUrl.value,login);notice.value=login?'已打开起点官方书架登录入口，请在官网完成登录；窗口打开不代表登录成功。':'官网窗口已打开，阅读权限与已购正文仍待验收。';});}
function close(id:string,remove:boolean){void action(async()=>{await closeAccount(id,remove);notice.value=remove?'已清理会话并移除本地记录，书库内容保留。':'已关闭并清理本地会话，再次打开需要重新登录。';});}
onMounted(()=>{void action(reload);});
</script>
<template>
  <section class="account-panel" aria-label="平台账户">
    <h2>平台账户 · 实验性</h2>
    <p>在平台官方窗口自行登录。每个账户窗口独立，关闭即结束会话；本软件不接收密码或 Cookie。窗口打开不代表已认证。起点支持当前可读章节的预览确认保存实验，真实已购场景尚待实测。</p>
    <form class="account-form" @submit.prevent="add"><label>平台<select v-model="platform" :disabled="busy"><option value="qidian">起点中文网</option><option value="shuqi">书旗小说</option></select></label><label>本地别名<input v-model="label" maxlength="40" placeholder="例如 我的起点账户，请勿填写凭据" :disabled="busy" required></label><button :disabled="busy||!label.trim()">添加账户记录</button></form>
    <label>打开的官网作品链接（可选）<input v-model="bookUrl" type="url" placeholder="https://www.qidian.com/book/作品ID/" :disabled="busy"></label>
    <p class="footnote">起点请点“登录起点”，通过官方书架进入独立登录页，不依赖首页登录弹层。填写作品链接后可用“打开官网窗口”查看作品；留空时起点进入登录入口，书旗打开首页。跨站登录方式可能被阻止，退出仅清理本机会话。</p>
    <p v-if="error" class="message failure" role="alert">{{ error }}</p><p v-if="notice" class="message success" role="status">{{ notice }}</p>
    <button :disabled="busy" @click="action(reload)">刷新会话状态</button>
    <p v-if="!accounts.length">尚无本地账户记录。</p>
    <ul class="accounts"><li v-for="row in accounts" :key="row.account.id"><h3>{{ row.account.label }} <small>{{ row.account.platform==='qidian'?'起点':'书旗' }}</small></h3><p>{{ row.session==='closed'?'会话已关闭':'官网窗口已打开 · 登录与权限未验证' }}</p><p v-if="row.blocked_navigation" class="message failure">平台尝试了当前不支持的导航，已阻止。可关闭窗口后重新打开；不要将凭据复制到软件中。</p><div class="actions"><button v-if="row.account.platform==='qidian'" class="primary" :disabled="busy" @click="open(row.account.id,true)">登录起点</button><button :disabled="busy" @click="open(row.account.id)">打开官网窗口</button><button :disabled="busy||row.session==='closed'" @click="close(row.account.id,false)">关闭并清理会话</button><button :disabled="busy" @click="close(row.account.id,true)">移除记录并清理会话</button></div><AutoSavePanel v-if="row.account.platform==='qidian'" :account-id="row.account.id" :opened="row.session!=='closed'" :preferred-book="selectedBook||props.bookId||''" :revision="catalogRevision" @saved="emit('saved',$event)" @read="emit('read',$event)" /><CapturePanel v-if="row.account.platform==='qidian'" :account-id="row.account.id" :opened="row.session!=='closed'" @saved="savedBook" @read="emit('read',$event)" /></li></ul>
  </section>
</template>
<style scoped>
.account-panel{background:#fffefa;border:1px solid #e1e5da;border-radius:14px;padding:28px}.account-form,.actions{display:flex;flex-wrap:wrap;align-items:end;gap:12px;margin:18px 0}.account-form label{flex:1;min-width:220px}.accounts{list-style:none;padding:0}.accounts li{border-top:1px solid #e1e5da;padding:16px 0}.accounts small{font-size:13px;color:#748069}.account-panel>p{line-height:1.8}.account-panel label{display:grid;gap:8px;font-size:13px;color:#6d7b61}.account-panel input,.account-panel select{width:100%;min-width:0;border:1px solid #d4ddca;background:#fafbf7;border-radius:5px;padding:11px;color:#334431;font:inherit}.account-form label:first-child{flex:0.55}.account-form button{min-height:42px}
</style>
