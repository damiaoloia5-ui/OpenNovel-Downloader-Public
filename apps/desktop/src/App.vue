<script setup lang="ts">
import {version as appVersion} from '../package.json';
import AppIcon from './AppIcon.vue';
import SaveLocation from './SaveLocation.vue';
import SaveOptionsPanel from './SaveOptions.vue';
import {defaultSaveOptions,resolveSaveRange,formatLabel,type SaveOptions} from './saveOptions';
import {setBookExportDirectory} from './api';
import {computed,nextTick,onMounted,onUnmounted,ref,watch} from 'vue';
import {getCurrentWindow} from '@tauri-apps/api/window';
import LibraryTools from './LibraryTools.vue';
import SourceSearch from './SourceSearch.vue';
import BatchPanel from './BatchPanel.vue';
import AccountPanel from './AccountPanel.vue';
import HelpGuide from './HelpGuide.vue';
import LocalEnginePanel from './LocalEnginePanel.vue';
import BookWorkspace from './BookWorkspace.vue';
import {automaticTxtSnapshot,localEngineStatus,setLocalEngine,type AutoTxtStatus,type LocalEngineStatus} from './api';
import {catalogAvailability} from './availabilityModel';
import {qidianBookLink,qidianBookId,officialBookPlatform} from './workflow';
import type {WorkbenchTab} from './workbenchModel';
import {preferences,sourceCapabilities,cancelJob,desktopAvailable,downloadBook,exportJob,importBook,inspectBook,jobDetails,pauseJob,resumeJob,retryChapters,revealExport,snapshot,type BookPreferences,type SourceInfo,type JobDetails,type Snapshot,type Catalog} from './api';

const page=ref<'download'|'library'|'tasks'|'sources'|'tools'|'accounts'|'help'|'book'>('download');
const pageHeading=ref<HTMLElement|null>(null);
const workspaceBook=ref('');const workspaceTab=ref<WorkbenchTab>('overview');const workspaceAccount=ref('');
const saveOptions=ref<SaveOptions>(defaultSaveOptions());const optionsTouched=ref(false);
const saveDirectory=ref<string|null>(null);const choosingDirectory=ref(false);const directoryTouched=ref(false);
const accountUrl=ref('');const bookInput=ref('');const inspectedInput=ref('');const catalog=ref<Catalog|null>(null);
const engineStatus=ref<LocalEngineStatus|null>(null);
const engineOffer=computed(()=>catalog.value?.source==='fanqie-web'&&catalog.value.chapters.some(c=>c.restricted)&&(engineStatus.value?.ready||engineStatus.value?.installed)&&!engineStatus.value?.enabled);
const sourceInfos=ref<SourceInfo[]>([]);const bookPreferences=ref<BookPreferences[]>([]);
const preferencesByBook=computed(()=>new Map(bookPreferences.value.map(p=>[p.book_id,p])));
const catalogSource=computed(()=>sourceInfos.value.find(s=>s.id===catalog.value?.source));
const catalogCanDownload=computed(()=>catalogSource.value?.can_download??false);
const firstChapter=ref(1);const lastChapter=ref(1);const selection=ref('');
const access=computed(()=>catalog.value?catalogAvailability(catalog.value,sourceInfos.value):null);
const automaticFiles=ref<AutoTxtStatus[]>([]);
const data=ref<Snapshot>({books:[],jobs:[]});const error=ref('');const notice=ref('');const working=ref(false);
const filter=ref('');const archiveFilter=ref('active');const fileInput=ref<HTMLInputElement|null>(null);
const openedJob=ref('');const details=ref<JobDetails|null>(null);const chapterFilter=ref('missing');const detailPage=ref(1);
const filteredItems=computed(()=>details.value?.items.filter(i=>chapterFilter.value==='all'||(chapterFilter.value==='missing'?i.state!=='completed':i.state===chapterFilter.value))??[]);
const detailPages=computed(()=>Math.max(1,Math.ceil(filteredItems.value.length/100)));
const visibleItems=computed(()=>filteredItems.value.slice((Math.min(detailPage.value,detailPages.value)-1)*100,Math.min(detailPage.value,detailPages.value)*100));
const chapterLabels:Record<string,string>={queued:'待下载',completed:'已保存',failed:'失败',blocked:'网页受限'};
const errorLabels:Record<string,string>={network:'网络请求',restricted:'访问限制',source_changed:'来源结构变化',invalid_content:'正文校验',io:'读写错误',other:'其他错误'};
const books=computed(()=>data.value.books.filter(b=>{const p=preferencesByBook.value.get(b.id);return (archiveFilter.value==='all'||(archiveFilter.value==='archived'?!!p?.archived:!p?.archived))&&`${b.title} ${b.author} ${p?.tags.join(' ')||''}`.toLocaleLowerCase().includes(filter.value.toLocaleLowerCase());}));
const bookPage=ref(1);const bookPages=computed(()=>Math.max(1,Math.ceil(books.value.length/48)));const visibleBooks=computed(()=>books.value.slice((bookPage.value-1)*48,bookPage.value*48));
const taskPage=ref(1);const taskFilter=ref('all');
const filteredJobs=computed(()=>data.value.jobs.filter(j=>taskFilter.value==='all'||(taskFilter.value==='active'?['queued','running','paused'].includes(j.state):taskFilter.value==='attention'?['failed','partial','blocked','cancelled'].includes(j.state):j.state==='completed')));
const taskPages=computed(()=>Math.max(1,Math.ceil(filteredJobs.value.length/20)));const visibleJobs=computed(()=>filteredJobs.value.slice((taskPage.value-1)*20,taskPage.value*20));
const saved=computed(()=>data.value.books.reduce((n,b)=>n+b.saved_chapters,0));const active=computed(()=>data.value.jobs.filter(j=>['queued','running'].includes(j.state)).length);
const labels:Record<string,string>={queued:'等待执行',running:'正在保存',paused:'中断待继续',cancelled:'已取消',failed:'执行失败',partial:'部分保存',blocked:'网页访问受限',completed:'所选范围完成'};
let navigationEpoch=0;let timer:ReturnType<typeof setTimeout>|undefined;let disposed=false;let refreshing=false;let minimized=false;let unlistenResize:(()=>void)|undefined;
watch(page,async value=>{navigationEpoch++;if(value==='help'){notice.value='';error.value='';}await nextTick();pageHeading.value?.focus();},{flush:'sync'});
watch([filter,archiveFilter],()=>{bookPage.value=1;});watch(bookPages,n=>{bookPage.value=Math.min(bookPage.value,n);});
watch(taskFilter,()=>{taskPage.value=1;});watch(taskPages,n=>{taskPage.value=Math.min(taskPage.value,n);});
function openWorkspace(id:string,tab:WorkbenchTab='overview',account=''){workspaceBook.value=id;workspaceTab.value=tab;workspaceAccount.value=account;error.value='';notice.value='';page.value='book';}
function showBook(id:string){openWorkspace(id,'save');}
function openTxt(id:string){const exportId=automaticFiles.value.find(f=>f.book_id===id&&f.state==='ready')?.export_id;if(exportId)void action(()=>revealExport(exportId));}
function openAccountBook(id:string){openWorkspace(id,'save');}
async function refresh(){
  if(!desktopAvailable||refreshing)return;refreshing=true;
  try{const [s,p,a]=await Promise.all([snapshot(),preferences(),automaticTxtSnapshot()]);if(disposed)return;data.value=s;bookPreferences.value=p;automaticFiles.value=a;await refreshDetails();}
  catch(e){error.value=String(e);}finally{refreshing=false;schedulePoll();}
}
function schedulePoll(){if(!disposed){if(timer)clearTimeout(timer);timer=setTimeout(()=>void poll(),active.value&&!document.hidden&&!minimized?800:5000);}}
async function poll(){if(disposed)return;if(document.hidden){schedulePoll();return;}try{minimized=await getCurrentWindow().isMinimized();if(disposed)return;if(minimized)schedulePoll();else await refresh();}catch(e){error.value=String(e);schedulePoll();}}
function visibilityChanged(){if(!document.hidden){if(timer)clearTimeout(timer);void poll();}}
async function refreshDetails(){const id=openedJob.value;if(!id)return;const result=await jobDetails(id);if(openedJob.value===id)details.value=result;}
async function showDetails(id:string){if(openedJob.value===id){openedJob.value='';details.value=null;return;}openedJob.value=id;details.value=null;detailPage.value=1;chapterFilter.value='missing';await action(refreshDetails);}
async function retryChapter(id:string,chapter:string){await action(async()=>{await retryChapters(id,[chapter]);notice.value='只重试所选失败章节，其他章节保持原状态。';});}
async function saveJob(id:string,format:'txt'|'epub'){await action(async()=>{const r=await exportJob(id,format);notice.value=`本次任务选章已导出（${r.saved_chapters}/${r.total_chapters} 章${r.partial?'，部分内容':''}）：${r.path}`;});}
async function action(work:()=>Promise<void>){if(working.value)return;working.value=true;error.value='';notice.value='';try{await work();await refresh();}catch(e){error.value=String(e);}finally{working.value=false;}}
async function openCreatedJob(id:string,epoch:number){try{const result=await jobDetails(id);if(disposed||epoch!==navigationEpoch)return;openWorkspace(result.job.book_id,'save');notice.value='任务已创建，保存结束后会按所选格式自动生成文件；请在本书工作台查看。';}catch{if(disposed||epoch!==navigationEpoch)return;page.value='tasks';notice.value='任务已创建，暂时无法读取作品信息，请在任务进度中查看。';}}
async function chooseFile(event:Event){const input=event.target as HTMLInputElement;const file=input.files?.[0];if(!file)return;await action(async()=>{if(file.size>16*1024*1024)throw new Error('文件超过 16 MiB，请拆分后导入。');const epoch=navigationEpoch;const id=await importBook(await file.text(),{directory:saveDirectory.value,format:saveOptions.value.format});await openCreatedJob(id,epoch);});input.value='';}
async function inspect(autoSave=false){
  if(choosingDirectory.value)return;
  try{resolveSaveRange(saveOptions.value);}catch(e){error.value=String(e);return;}
  if(officialBookPlatform(bookInput.value)){accountUrl.value=bookInput.value.trim();page.value='accounts';error.value='';notice.value=officialBookPlatform(bookInput.value)==='migu'?'打开这本书，加载完成后识别官网试读；公开试读无需登录。':'在平台官网窗口登录，打开可正常阅读的章节，再识别当前页。';return;}
  const link=qidianBookLink(bookInput.value);if(link){const stored=data.value.books.find(b=>b.source==='qidian-session'&&b.remote_id===qidianBookId(link));if(stored){await action(async()=>{if(directoryTouched.value)await setBookExportDirectory(stored.id,saveDirectory.value);openWorkspace(stored.id,'save');});return;}accountUrl.value=link;page.value='accounts';error.value='';notice.value='按提示完成登录和书名确认，之后会自动保存并生成 TXT。';return;}
  await action(async()=>{
    const epoch=navigationEpoch;catalog.value=null;const input=bookInput.value.trim();
    let result=await inspectBook(input);if(disposed||epoch!==navigationEpoch||bookInput.value.trim()!==input)return;
    if(result.source==='fanqie-web'&&result.chapters.some(c=>c.restricted)){
      if(!engineStatus.value||engineStatus.value.remembered&&!engineStatus.value.enabled){try{engineStatus.value=await localEngineStatus();}catch{engineStatus.value=null;}}
      if(disposed||epoch!==navigationEpoch||bookInput.value.trim()!==input)return;
      if(autoSave&&engineStatus.value?.remembered&&engineStatus.value.installed&&!engineStatus.value.ready){notice.value='正在准备本地增强服务，首次准备可能需要几分钟…';engineStatus.value=await setLocalEngine(true);}
      if(disposed||epoch!==navigationEpoch||bookInput.value.trim()!==input)return;
      if(engineStatus.value?.enabled){result=await inspectBook(input);sourceInfos.value=await sourceCapabilities();}
    }
    if(!sourceInfos.value.length)sourceInfos.value=await sourceCapabilities();
    if(disposed||epoch!==navigationEpoch||bookInput.value.trim()!==input)return;
    const stored=data.value.books.find(b=>b.source===result.source&&b.remote_id===result.remote_id);
    if(autoSave&&stored&&!optionsTouched.value){if(directoryTouched.value)await setBookExportDirectory(stored.id,saveDirectory.value);openWorkspace(stored.id,'save');return;}
    catalog.value=result;inspectedInput.value=input;firstChapter.value=1;lastChapter.value=result.chapters.length;selection.value='';
    if(autoSave&&result.source==='fanqie-web'&&result.chapters.some(c=>c.restricted)){
      if(!engineStatus.value){try{engineStatus.value=await localEngineStatus();}catch{engineStatus.value=null;}}
      if(disposed||epoch!==navigationEpoch)return;
      if(engineOffer.value){notice.value='这本书有网页受限章节。可启用本地增强，尝试更多明确免费的章节。';return;}
      if(engineStatus.value?.remembered&&!engineStatus.value.ready){notice.value=engineStatus.value.message;return;}
    }
    if(autoSave){const range=catalogAvailability(result,sourceInfos.value);if(!range.supported||!range.open){notice.value='已识别目录，当前没有可自动保存的开放章，请查看下方范围说明。';return;}
      if(disposed||epoch!==navigationEpoch)return;const chosen=resolveSaveRange(saveOptions.value,result.chapters.length);const id=await downloadBook(input,1,result.chapters.length,chosen||range.selection,{directory:saveDirectory.value,format:saveOptions.value.format,selection_only:!!chosen});await openCreatedJob(id,epoch);}
  });
}
async function selectSearchResult(input:string){bookInput.value=input;await inspect();}
async function startDownload(openOnly=false){await action(async()=>{if(!catalogCanDownload.value)throw new Error(catalogSource.value?.note||'该来源暂不支持正文下载');if(bookInput.value.trim()!==inspectedInput.value)throw new Error('链接已改变，请重新读取目录。');const range=resolveSaveRange(saveOptions.value,catalog.value!.chapters.length);const chosen=selection.value.trim()||range||(openOnly?access.value?.selection:'');if(openOnly&&!chosen)throw new Error('当前目录没有标记开放的章节。');const epoch=navigationEpoch;const id=await downloadBook(inspectedInput.value,firstChapter.value,lastChapter.value,chosen,{directory:saveDirectory.value,format:saveOptions.value.format,selection_only:!!range||!!selection.value.trim()});await openCreatedJob(id,epoch);});}
function sourceLabel(id:string){const info=sourceInfos.value.find(s=>s.id===id);return id==='local-json'?'本地 JSON':info?`${info.name} · 实验性`:id;}
function coverTone(id:string){let hash=0;for(const char of id)hash=(hash*31+char.charCodeAt(0))>>>0;return ['jade','slate','clay','plum','blue'][hash%5];}
async function engineChanged(){try{const next=await sourceCapabilities();if(!working.value&&sourceInfos.value.find(s=>s.id==='fanqie-app-local')?.can_download!==next.find(s=>s.id==='fanqie-app-local')?.can_download){catalog.value=null;inspectedInput.value='';}sourceInfos.value=next;}catch(e){error.value=String(e);}}
async function enableEngineForBook(){const input=inspectedInput.value;const epoch=navigationEpoch;let enabled=false;await action(async()=>{notice.value='正在准备本地增强服务，请稍候…';const current=await setLocalEngine(true);engineStatus.value=current;if(!current.enabled)throw new Error(current.message);sourceInfos.value=await sourceCapabilities();enabled=true;});if(enabled&&!disposed&&epoch===navigationEpoch&&bookInput.value.trim()===input)await inspect(true);}
async function openEngineBook(input:string){bookInput.value=input;page.value='download';await nextTick();await inspect();}
onMounted(()=>{if(!desktopAvailable)return;void localEngineStatus().then(async value=>{if(disposed)return;engineStatus.value=value;await engineChanged();}).catch(()=>{if(!disposed)engineStatus.value={enabled:false,ready:false,message:'增强组件检查暂不可用，请重新检查；普通网页保存仍可使用。'};});void sourceCapabilities().then(r=>sourceInfos.value=r).catch(e=>error.value=String(e));void getCurrentWindow().onResized(visibilityChanged).then(unlisten=>{if(disposed)unlisten();else unlistenResize=unlisten;}).catch(e=>error.value=String(e));void poll();document.addEventListener('visibilitychange',visibilityChanged);});
onUnmounted(()=>{disposed=true;if(timer)clearTimeout(timer);unlistenResize?.();document.removeEventListener('visibilitychange',visibilityChanged);});
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <a class="brand" href="#" @click.prevent="page = 'library'">
        <span class="brand-mark" aria-hidden="true"><AppIcon name="book" :size="25" /></span>
        <span>OpenNovel<small>属于你的本地书库</small></span>
      </a>
      <div class="nav-label">开始使用</div>
      <nav aria-label="主导航">
        <button :class="{ selected: page === 'download' }" :aria-current="page==='download'?'page':undefined" @click="page = 'download'"><AppIcon name="plus" />添加小说</button>
        <button :class="{ selected: page === 'library' || page === 'book' }" :aria-current="['library','book'].includes(page)?'page':undefined" @click="page = 'library'"><AppIcon name="library" />我的书库<b>{{ data.books.length }}</b></button>
        <button :class="{ selected: page === 'tasks' }" :aria-current="page==='tasks'?'page':undefined" @click="page = 'tasks'"><AppIcon name="download" />保存进度<b v-if="active">{{ active }}</b></button>
        <button :class="{ selected: page === 'help' }" :aria-current="page==='help'?'page':undefined" @click="page='help'"><AppIcon name="help" />使用指引</button>
        <details class="nav-more" :open="['sources','accounts','tools'].includes(page)">
<summary>更多功能</summary>
        <button :class="{ selected: page === 'sources' }" @click="page = 'sources'"><AppIcon name="globe" />小说来源</button>
        <button :class="{ selected: page === 'accounts' }" @click="page='accounts'"><AppIcon name="user" />平台账户</button>
        <button :class="{ selected: page === 'tools' }" @click="page = 'tools'"><AppIcon name="archive" />备份与诊断</button>
        </details>
      </nav>
      <div class="sidebar-note"><span class="dot"></span>本地保存，离线导出<p>OpenNovel Downloader · {{ appVersion }}</p></div>
    </aside>

    <main>
      <header class="topbar"><span class="breadcrumb"><span>个人工作空间</span><span aria-hidden="true">/</span><strong>{{ page==='book'?'作品工作台':page==='help'?'使用指引':page === 'download' ? '添加小说' : page === 'library' ? '书库' : page === 'tasks' ? '任务' : page==='tools' ? '备份与诊断' : page==='accounts'?'账户':'来源' }}</strong></span><button class="help-entry" @click="page='help'"><AppIcon name="help" :size="16" />不会操作？查看指引</button></header>
      <div class="main-content">
        <div v-if="!desktopAvailable" class="message warning" role="status">当前是浏览器界面预览。请运行桌面应用以使用本地书库和导出功能。</div>
        <div v-if="error" class="message failure" role="alert">{{ error }}<button aria-label="关闭错误提示" @click="error = ''">×</button></div>
        <div v-if="notice" class="message success" role="status">{{ notice }}<button aria-label="关闭提示" @click="notice = ''">×</button></div>
        <div v-if="page!=='book'" class="page-heading">
          <div><h1 ref="pageHeading" tabindex="-1">{{ page==='help'?'使用指引':page === 'download' ? '添加小说' : page === 'library' ? '我的书库' : page === 'tasks' ? '保存进度' : page==='accounts'?'登录与保存正文':page==='tools'?'备份与诊断':'小说来源' }}</h1><p class="intro">{{ page==='help'?'从添加到导出，按步骤完成。':page === 'download' ? '只需粘贴小说链接，软件会带你完成，先选章节、格式和位置，再开始保存。' : page === 'library' ? '选择作品，阅读或导出已经保存的正文。' : page === 'tasks' ? '查看保存进度，继续中断的任务。' : page==='accounts'?'选择平台，登录官网并核对目录；仅保存实际可读正文，已购能力仍需实测。':page==='tools'?'创建书库备份、恢复副本，查看诊断信息。':'查看各个平台已验证的支持范围。' }}</p></div>
          <button v-if="page==='library'" class="primary" @click="page='download'"><AppIcon name="plus" :size="18" />添加一本小说</button>
          <input ref="fileInput" type="file" accept=".json,application/json" class="file-input" aria-label="选择 JSON 书籍文件" @change="chooseFile" />
        </div>

        <template v-if="page === 'download'">
          <ol class="quick-path" aria-label="基本操作步骤"><li><b>1</b> 粘贴链接</li><li><b>2</b> 选择保存选项</li><li><b>3</b> 拿到文件</li></ol>
          <form class="download-panel link-entry" @submit.prevent="inspect(true)">
            <span class="entry-symbol"><AppIcon name="link" :size="26" /></span>
            <label for="novel-link">粘贴小说的官网作品链接</label>
            <div class="link-row"><input id="novel-link" v-model="bookInput" type="text" placeholder="粘贴官网作品链接，例如纵横、17K、番茄、七猫、晋江或起点" :disabled="working" required /></div>
            <SaveOptionsPanel v-model="saveOptions" :disabled="working" :total="bookInput.trim()===inspectedInput?catalog?.chapters.length:undefined" @update:model-value="optionsTouched=true" /><SaveLocation v-model="saveDirectory" :disabled="working" @busy="choosingDirectory=$event" @update:model-value="directoryTouched=true" />
            <button class="primary" :disabled="!desktopAvailable || working || choosingDirectory || !bookInput.trim()">{{ working ? '正在识别，请稍等…' : `开始保存为 ${formatLabel(saveOptions.format)}` }}</button>
            <p class="footnote">免费网页会直接开始保存；起点会提示你登录。完成后按上方选择自动生成 TXT 或 Word。</p><p class="footnote">不知道复制哪个链接？打开小说的作品介绍页，复制浏览器顶部地址，粘贴到这里。书旗可保存官网明确开放的免费章节，其他章节会标注未保存。</p>
            <details class="optional-options"><summary>只想先看看能保存多少？</summary><button type="button" :disabled="!desktopAvailable||working||!bookInput.trim()" @click="inspect()">只看获取范围</button></details>
          </form>
          <LocalEnginePanel :disabled="working" :current="engineStatus" @checked="engineStatus=$event" @changed="engineChanged" />
          <section v-if="catalog" class="download-panel" aria-label="作品目录">
            <h2>{{ catalog.title }}</h2><p class="footnote">{{ catalogSource?.name || catalog.source }} · {{ catalog.author || '未标注作者' }} · 全目录 {{ catalog.chapters.length }} 章</p>
            <section v-if="engineOffer" aria-label="更多免费章节保存方式"><h3>启用一次，之后直接保存免费章节</h3><p class="footnote">组件已由软件管理。首次启用会检查环境，必要时启动已安装的隔离服务，并记住选择；只保存明确免费且通过校验的正文。</p><p v-if="!engineStatus?.ready&&engineStatus?.next_step" class="footnote">下一步：{{ engineStatus.next_step }}</p><button class="primary" :disabled="working||bookInput.trim()!==inspectedInput" @click="enableEngineForBook">启用本地组件，保存更多免费章节</button><p class="footnote">也可以继续使用下方的“仅保存目录标记开放章”。</p></section>
            <section v-if="access" aria-label="目录可获取范围"><h3>先看当前获取范围</h3><p v-if="access.supported" role="status">目录标记开放 {{ access.open }} 章 · 目录受限 {{ access.restricted }} 章。开放标记仅表示可以尝试，不代表正文已验证，也不保证整书免费。</p><p v-else role="status">正文待验证 {{ access.unverified }} 章，当前来源没有开放网页正文下载。</p><p class="footnote">受限可能涉及付费、锁定或渠道限制；实际结果以逐章保存为准。不会自动购买、广告解锁或切换来源。</p></section>
            <p v-if="!catalogCanDownload" class="message warning" role="status">{{ catalogSource?.note || '该来源暂不支持正文下载' }}</p>
            <p v-if="catalog.description" class="book-description">{{ catalog.description }}</p>
            <button class="primary" :disabled="working||choosingDirectory||!access?.open||bookInput.trim()!==inspectedInput" @click="startDownload(true)">仅保存目录标记开放章（{{ access?.open||0 }}）</button>
            <details class="optional-options"><summary>选章设置与目录预览（可选）</summary><label class="selection-label">指定序号（可选）<input v-model="selection" type="text" placeholder="例如 1,3,5-8" /></label><button :disabled="working||choosingDirectory||!catalogCanDownload||bookInput.trim()!==inspectedInput||(!selection.trim()&&(!Number.isInteger(firstChapter)||!Number.isInteger(lastChapter)||firstChapter<1||lastChapter<firstChapter||lastChapter>catalog.chapters.length))" @click="startDownload()">保存自定义范围（包含受限标记时会明确记录）</button><ol class="catalog-list"><li v-for="chapter in catalog.chapters.slice(0, 100)" :key="chapter.id"><span>{{ chapter.title }}</span><small>{{ !catalogCanDownload ? '正文访问未验证' : chapter.restricted ? '网页受限' : '网页标记开放' }}</small></li></ol><p v-if="catalog.chapters.length>100" class="footnote">预览前 100 章；保存范围可选择完整目录。</p></details>
          </section>
          <details class="optional-options"><summary>其他方式：按书名搜索或导入本地文件</summary><SourceSearch :sources="sourceInfos" :busy="working" @select="selectSearchResult" /><button :disabled="!desktopAvailable||working||choosingDirectory" @click="fileInput?.click()">选择本地 JSON 文件</button><p class="footnote">JSON 会完整导入，使用上方所选格式和保存位置；如需部分章节，可导入后到“其他格式与文件记录”按范围导出。</p></details>
        </template>

        <template v-if="page === 'library'">
          <section class="stats" aria-label="书库统计">
            <div><span>本地作品</span><strong>{{ data.books.length }}<small>本</small></strong></div>
            <div><span>已保存章节</span><strong>{{ saved }}<small>章</small></strong></div>
            <div><span>运行中任务</span><strong>{{ active }}<small>项</small></strong></div>
          </section>
          <div class="section-heading"><h2>我的书架</h2><select v-model="archiveFilter" aria-label="书架范围"><option value="active">书架</option><option value="archived">已归档</option><option value="all">全部作品</option></select><input v-model="filter" type="search" placeholder="搜索书名、作者或标签" aria-label="搜索书库" /></div>
          <section v-if="books.length" class="book-grid" aria-label="书籍列表">
            <article v-for="book in visibleBooks" :key="book.id" class="book-card" :data-tone="coverTone(book.id)">
              <div class="book-cover"><span class="cover-source">{{ sourceInfos.find(s=>s.id===book.source)?.name||(book.source==='local-json'?'本地作品':'本地收藏') }}</span><h3 :title="book.title">{{ book.title }}</h3><p>{{ book.author || '未标注作者' }}</p><div class="cover-signature"><AppIcon name="book" :size="17" /><span>OpenNovel</span></div></div>
              <div class="book-info">
<span class="source-tag">{{ sourceLabel(book.source) }}</span><h3 :title="book.title">{{ book.title }}</h3><p>{{ book.saved_chapters }} / {{ book.total_chapters }} 章<span class="coverage-badge" :class="{complete:book.saved_chapters===book.total_chapters}">{{ book.saved_chapters === book.total_chapters ? '完整保存' : '部分保存' }}</span></p><progress class="book-progress" :value="book.saved_chapters" :max="book.total_chapters" :aria-label="`${book.title}全书保存进度`"></progress>
                <p v-if="preferencesByBook.get(book.id)?.tags.length" class="footnote">{{ preferencesByBook.get(book.id)?.tags.join(' · ') }}</p>
                <div class="book-actions"><button :disabled="working" @click="showBook(book.id)">查看进度与正文</button><button v-if="automaticFiles.some(f=>f.book_id===book.id&&f.state==='ready'&&f.export_id)" class="primary" :disabled="working" @click="openTxt(book.id)">打开文件夹</button></div>
</div>
            </article>
          </section>
          <div v-else class="empty-state"><span class="empty-icon" aria-hidden="true"><AppIcon name="library" :size="35" /></span><h2>{{ filter ? '没有找到这本书' : archiveFilter === 'archived' ? '还没有归档作品' : '从第一本书开始' }}</h2><p>{{ filter ? '试试其他书名、作者或标签。' : archiveFilter === 'archived' ? '在作品详情中归档后，会显示在这里。' : '点“添加一本小说”，粘贴官网链接，按提示保存即可。' }}</p><p v-if="!filter && archiveFilter !== 'archived'" class="muted">保存完成后会自动生成 TXT，点“打开文件夹”即可找到。</p></div>
          <nav v-if="books.length" class="list-pagination" aria-label="书库分页"><span>共 {{ books.length }} 本 · 第 {{ bookPage }} / {{ bookPages }} 页</span><button :disabled="bookPage <= 1" @click="bookPage = 1">首页</button><button :disabled="bookPage <= 1" @click="bookPage--">上一页</button><button :disabled="bookPage >= bookPages" @click="bookPage++">下一页</button><button :disabled="bookPage >= bookPages" @click="bookPage = bookPages">末页</button></nav>
        </template>

        <template v-if="page === 'tasks'">
          <details class="optional-options"><summary>批量添加多部作品（高级）</summary><BatchPanel /></details>
          <div class="section-heading"><h2>保存记录 <span>{{ data.jobs.length }}</span></h2><select v-model="taskFilter" aria-label="任务筛选"><option value="all">全部任务</option><option value="active">进行中与暂停</option><option value="attention">失败、部分或受限</option><option value="completed">已完成</option></select><button class="subtle" :disabled="!desktopAvailable" @click="refresh">刷新状态</button></div>
          <div v-if="!data.jobs.length" class="empty-state"><span class="empty-icon" aria-hidden="true">↓</span><h2>还没有任务</h2><p>添加小说链接或导入本地内容，任务进度会保存在书库中。</p></div>
          <p v-else-if="!filteredJobs.length" class="footnote">当前筛选下没有任务。</p>
          <article v-for="job in visibleJobs" :key="job.id" class="task-card" :data-state="job.state">
            <div class="task-heading"><h3>{{ job.title }}</h3><span class="state-tag">{{ labels[job.state] || job.state }}</span></div>
            <progress :value="job.saved" :max="job.total" :aria-label="`${job.title}保存进度`"></progress>
            <div class="task-bottom"><span>本次范围已保存 {{ job.saved }} / {{ job.total }} 章 · 受限 {{ job.blocked }} · 失败 {{ job.failed }}</span><div class="task-controls"><button v-if="job.state === 'running' || job.state === 'queued'" :disabled="working" @click="action(() => pauseJob(job.id))">暂停</button><button v-if="['running','queued','paused'].includes(job.state)" :disabled="working" @click="action(() => cancelJob(job.id))">取消任务</button><button v-if="job.source.endsWith('-session') && !['running','queued','completed'].includes(job.state)" :disabled="working" @click="openAccountBook(job.book_id)">继续保存</button><button v-if="!job.source.endsWith('-session') && !['running','queued','completed','blocked'].includes(job.state) && (job.state !== 'partial' || job.failed || job.saved + job.blocked < job.total)" :disabled="working" @click="action(() => resumeJob(job.id))">继续 / 重试</button></div></div>
            <div v-if="job.saved" class="task-controls task-export"><button :disabled="working" @click="showBook(job.book_id)">阅读与导出本书</button><details><summary>仅导出本次任务选章（高级）</summary><button :disabled="working" @click="saveJob(job.id,'txt')">导出本次选章 TXT</button><button :disabled="working" @click="saveJob(job.id,'epub')">导出本次选章 EPUB</button></details></div>
            <p v-if="job.error" class="task-error">{{ job.error }}</p>
            <button class="detail-toggle" :aria-expanded="openedJob === job.id" :disabled="working" @click="showDetails(job.id)">{{ openedJob === job.id ? '收起章节明细' : '查看章节明细' }}</button>
            <section v-if="openedJob === job.id" class="chapter-details" :aria-label="`${job.title}章节明细`">
              <p class="footnote">本任务的执行记录；“网页受限”不能直接重试。可在书库检查更新，重新确认官网开放状态。</p>
              <div class="detail-toolbar"><label>章节筛选<select v-model="chapterFilter" @change="detailPage = 1"><option value="missing">未保存</option><option value="all">全部</option><option value="failed">失败</option><option value="blocked">受限</option><option value="queued">待下载</option></select></label><span>{{ filteredItems.length }} 章</span></div>
              <p v-if="!details" role="status">正在读取章节明细…</p>
              <template v-else>
                <p v-if="!filteredItems.length" class="footnote">当前筛选下没有章节。</p>
                <div v-else class="chapter-table"><table><thead><tr><th>序号 / 章节</th><th>状态</th><th>原因与操作</th></tr></thead><tbody><tr v-for="item in visibleItems" :key="item.chapter_id"><td>{{ item.position }} · {{ item.title }}<small>{{ item.chapter_id }}</small></td><td>{{ chapterLabels[item.state] || item.state }}</td><td><span v-if="item.error_kind" class="error-kind">{{ errorLabels[item.error_kind] || item.error_kind }}</span><span>{{ item.error || (item.state === 'blocked' ? '旧任务未记录详情，请检查目录更新。' : item.state === 'failed' ? '旧任务未记录详情，可重试确认原因。' : '—') }}</span><button v-if="item.retryable" :disabled="working || job.state === 'running' || job.state === 'queued'" @click="retryChapter(job.id,item.chapter_id)">重试此章</button></td></tr></tbody></table></div>
                <div v-if="detailPages > 1" class="detail-pagination"><button :disabled="detailPage <= 1" @click="detailPage = Math.min(detailPage,detailPages)-1">上一页</button><span>{{ Math.min(detailPage,detailPages) }} / {{ detailPages }}</span><button :disabled="detailPage >= detailPages" @click="detailPage++">下一页</button></div>
              </template>
            </section>
          </article>
          <nav v-if="filteredJobs.length" class="list-pagination" aria-label="任务分页"><span>共 {{ filteredJobs.length }} 项 · 第 {{ taskPage }} / {{ taskPages }} 页</span><button :disabled="taskPage <= 1" @click="taskPage = 1">首页</button><button :disabled="taskPage <= 1" @click="taskPage--">上一页</button><button :disabled="taskPage >= taskPages" @click="taskPage++">下一页</button><button :disabled="taskPage >= taskPages" @click="taskPage = taskPages">末页</button></nav>
        </template>

        <template v-if="page === 'sources'">
          <div class="source-intro"><h2>已接入与待适配平台</h2><p>纵横、17K、番茄、七猫、晋江与书旗提供实验性官网免费正文保存；起点提供账户内保存实验。刺猬猫、飞卢已接入目录和账户连续保存实验；纵横、17K、晋江、七猫、书旗也可尝试官网账户通道。真实已购全书未验收；咪咕仅作品页识别，正文未接通。所选范围完成不代表整本完整。</p></div>
          <div class="source-list">
            <article><span class="source-monogram">本</span><div><h3>本地 JSON 内容</h3><p>用户提供的作品与章节 · 任务恢复 · TXT / EPUB</p></div><span class="available">已接入</span></article>
            <article v-for="source in sourceInfos" :key="source.id"><span class="source-monogram">{{ source.name.slice(0,1) }}</span><div><h3>{{ source.name }}</h3><p>{{ source.note }}</p><small>{{ source.example }}</small></div><span :class="source.can_download ? 'available' : 'pending-label'">{{ source.id==='qidian-session'?'账户内自动保存':['ciweimao-web','faloo-web','migu-web'].includes(source.id)?'账户当前页实验':source.can_inspect===false?'待适配':source.can_download ? '实验性下载' : '仅详情/目录' }}</span></article>
          </div>
          <p class="footnote">来源能力按目录、正文、账户与访问限制分别记录。付费平台留在后续路线，目前不承诺未购 VIP 正文获取能力。</p>
        </template>
        <LibraryTools v-if="page==='tools'" />
        <AccountPanel v-if="page==='accounts'" :initial-url="accountUrl" :initial-options="saveOptions" :initial-directory="directoryTouched?saveDirectory:undefined" @saved="refresh" @ready="(id,account)=>openWorkspace(id,'save',account)" @read="id=>openWorkspace(id,'read')" />
        <BookWorkspace v-if="page==='book'&&workspaceBook" :key="workspaceBook" :book-id="workspaceBook" :initial-tab="workspaceTab" :initial-options="optionsTouched?saveOptions:undefined" :account-id="workspaceAccount" :sources="sourceInfos" :jobs="data.jobs" :summary="data.books.find(b=>b.id===workspaceBook)" :preference="preferencesByBook.get(workspaceBook)" :automatic="automaticFiles.find(a=>a.book_id===workspaceBook)" @changed="refresh" @back="page='library'" @tasks="page='tasks'" @help="page='help'" @engine="openEngineBook" @account="accountUrl=$event;page='accounts'" />
        <HelpGuide v-if="page==='help'" @navigate="page=$event" @import="fileInput?.click()" />
        <footer>OpenNovel Downloader<span>开放的工具，长久的书库。</span></footer>
      </div>
    </main>
  </div>
</template>
