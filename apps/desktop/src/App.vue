<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { cancelJob, desktopAvailable, downloadBook, exportBook, importBook, inspectBook, jobDetails, resumeJob, retryChapters, snapshot, updateBook, type Catalog, type JobDetails, type Snapshot } from './api';

const page = ref<'download' | 'library' | 'tasks' | 'sources'>('download');
const bookInput = ref('');
const inspectedInput = ref('');
const catalog = ref<Catalog | null>(null);
const firstChapter = ref(1);
const lastChapter = ref(1);
const accessible = computed(() => catalog.value?.chapters.filter(chapter => !chapter.restricted).length ?? 0);
const data = ref<Snapshot>({ books: [], jobs: [] });
const error = ref('');
const notice = ref('');
const working = ref(false);
const filter = ref('');
const fileInput = ref<HTMLInputElement | null>(null);
const openedJob = ref('');
const details = ref<JobDetails | null>(null);
const chapterFilter = ref('missing');
const detailPage = ref(1);
const filteredItems = computed(() => details.value?.items.filter(item => chapterFilter.value === 'all' || (chapterFilter.value === 'missing' ? item.state !== 'completed' : item.state === chapterFilter.value)) ?? []);
const detailPages = computed(() => Math.max(1, Math.ceil(filteredItems.value.length / 100)));
const visibleItems = computed(() => filteredItems.value.slice((Math.min(detailPage.value, detailPages.value)-1)*100, Math.min(detailPage.value, detailPages.value)*100));
const chapterLabels: Record<string,string> = {queued:'待下载',completed:'已保存',failed:'失败',blocked:'网页受限'};
const errorLabels: Record<string,string> = {network:'网络请求',restricted:'访问限制',source_changed:'来源结构变化',invalid_content:'正文校验',io:'读写错误',other:'其他错误'};
const books = computed(() => data.value.books.filter(book => `${book.title} ${book.author}`.toLocaleLowerCase().includes(filter.value.toLocaleLowerCase())));
const saved = computed(() => data.value.books.reduce((total, book) => total + book.saved_chapters, 0));
const active = computed(() => data.value.jobs.filter(job => job.state === 'queued' || job.state === 'running').length);
const labels: Record<string, string> = { queued: '等待执行', running: '正在保存', paused: '中断待继续', cancelled: '已取消', failed: '执行失败', partial: '部分保存', blocked: '网页访问受限', completed: '所选范围完成' };
let timer: ReturnType<typeof setInterval> | undefined;
let refreshing = false;

async function refresh() {
  if (!desktopAvailable || refreshing) return;
  refreshing = true;
  try { data.value = await snapshot(); await refreshDetails(); }
  catch (reason) { error.value = String(reason); }
  finally { refreshing = false; }
}

async function refreshDetails() {
  const id = openedJob.value;
  if (!id) return;
  const result = await jobDetails(id);
  if (openedJob.value === id) details.value = result;
}

async function showDetails(id:string) {
  if (openedJob.value === id) { openedJob.value = ''; details.value = null; return; }
  openedJob.value = id; details.value = null; detailPage.value = 1; chapterFilter.value = 'missing';
  await action(refreshDetails);
}

async function retryChapter(jobId:string, chapterId:string) {
  await action(async () => { await retryChapters(jobId,[chapterId]); notice.value = '只重试所选失败章节；其他待下载章节保持原状态。'; });
}

async function checkUpdates(bookId:string) {
  await action(async () => { await updateBook(bookId); page.value = 'tasks'; notice.value = '已重新读取官网目录并创建补齐任务；已有章节复用，受限章节仍会明确显示。'; });
}

async function action(work: () => Promise<void>) {
  if (working.value) return;
  error.value = '';
  notice.value = '';
  working.value = true;
  try { await work(); await refresh(); }
  catch (reason) { error.value = String(reason); }
  finally { working.value = false; }
}

async function chooseFile(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  await action(async () => {
    if (file.size > 16 * 1024 * 1024) throw new Error('文件超过 16 MiB，请拆分后导入。');
    await importBook(await file.text());
    notice.value = '导入任务已创建，章节将逐一保存到本地。';
    page.value = 'tasks';
  });
  input.value = '';
}

async function inspect() {
  await action(async () => {
    catalog.value = null;
    const input = bookInput.value.trim();
    const result = await inspectBook(input);
    catalog.value = result;
    inspectedInput.value = input;
    firstChapter.value = 1;
    lastChapter.value = result.chapters.length;
  });
}

async function startDownload() {
  await action(async () => {
    if (bookInput.value.trim() !== inspectedInput.value) throw new Error('链接已改变，请重新读取目录。');
    await downloadBook(inspectedInput.value, firstChapter.value, lastChapter.value);
    page.value = 'tasks';
    notice.value = '下载任务已创建；网页受限章节会单独计数，不保存截断预览。';
  });
}

function sourceLabel(source: string) { return source === 'fanqie-web' ? '番茄官网网页 · 实验性' : '本地 JSON'; }

async function saveBook(id: string, format: 'txt' | 'epub') {
  await action(async () => {
    const receipt = await exportBook(id, format);
    notice.value = `${receipt.partial ? '部分内容' : '完整内容'}已导出（${receipt.saved_chapters}/${receipt.total_chapters} 章）：${receipt.path}`;
  });
}

onMounted(() => { void refresh(); timer = setInterval(() => void refresh(), 800); });
onUnmounted(() => { if (timer) clearInterval(timer); });
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <a class="brand" href="#" @click.prevent="page = 'library'">
        <span class="brand-mark" aria-hidden="true">O<span>N</span></span>
        <span>OpenNovel<small>属于你的本地书库</small></span>
      </a>
      <div class="nav-label">工作空间</div>
      <nav aria-label="主导航">
        <button :class="{ selected: page === 'download' }" @click="page = 'download'"><span aria-hidden="true">＋</span>添加小说</button>
        <button :class="{ selected: page === 'library' }" @click="page = 'library'"><span aria-hidden="true">▤</span>我的书库<b>{{ data.books.length }}</b></button>
        <button :class="{ selected: page === 'tasks' }" @click="page = 'tasks'"><span aria-hidden="true">↓</span>下载与导入<b v-if="active">{{ active }}</b></button>
        <button :class="{ selected: page === 'sources' }" @click="page = 'sources'"><span aria-hidden="true">◎</span>小说来源</button>
      </nav>
      <div class="sidebar-note"><span class="dot"></span>本地保存，离线导出<p>工程预览 · 0.0.1</p></div>
    </aside>

    <main>
      <header class="topbar"><span>个人工作空间 / {{ page === 'download' ? '添加小说' : page === 'library' ? '书库' : page === 'tasks' ? '任务' : '来源' }}</span><span class="version">开发预览</span></header>
      <div class="main-content">
        <div v-if="!desktopAvailable" class="message warning" role="status">当前是浏览器界面预览。请运行桌面应用以使用本地书库和导出功能。</div>
        <div v-if="error" class="message failure" role="alert">{{ error }}<button aria-label="关闭错误提示" @click="error = ''">×</button></div>
        <div v-if="notice" class="message success" role="status">{{ notice }}<button aria-label="关闭提示" @click="notice = ''">×</button></div>
        <div class="page-heading">
          <div><p class="eyebrow">YOUR STORIES, KEPT CLOSE</p><h1>{{ page === 'download' ? '从一条作品链接开始。' : page === 'library' ? '让故事留在身边。' : page === 'tasks' ? '每一章，都有着落。' : '一个书库，更多来源。' }}</h1><p class="intro">{{ page === 'download' ? '读取官网目录，保存网页实际开放的完整章节。' : page === 'library' ? '整理已经拥有的文字，随时导出、再次阅读。' : page === 'tasks' ? '查看真实保存进度，继续中断的任务。' : '平台能力逐项验证，支持范围持续公开。' }}</p></div>
          <button v-if="page !== 'sources'" class="primary" :disabled="!desktopAvailable || working" @click="fileInput?.click()">＋ 导入本地书籍</button>
          <input ref="fileInput" type="file" accept=".json,application/json" class="file-input" aria-label="选择 JSON 书籍文件" @change="chooseFile" />
        </div>

        <template v-if="page === 'download'">
          <form class="download-panel" @submit.prevent="inspect">
            <label for="novel-link">番茄作品详情页链接或作品 ID</label>
            <div class="link-row"><input id="novel-link" v-model="bookInput" type="text" placeholder="https://fanqienovel.com/page/作品ID" :disabled="working" required /><button class="primary" :disabled="!desktopAvailable || working">{{ working ? '处理中…' : '读取目录' }}</button></div>
            <p class="footnote">实验性网页来源。免费作品也可能有仅限 App 阅读的章节；目前未接入账户、付费或 App 正文。</p>
          </form>
          <section v-if="catalog" class="download-panel" aria-label="作品目录">
            <h2>{{ catalog.title }}</h2><p class="footnote">{{ catalog.author || '未标注作者' }} · 全目录 {{ catalog.chapters.length }} 章 · 网页标记开放 {{ accessible }} 章</p>
            <div class="range-row"><label>起始序号<input v-model.number="firstChapter" type="number" min="1" :max="catalog.chapters.length" /></label><label>结束序号<input v-model.number="lastChapter" type="number" :min="firstChapter" :max="catalog.chapters.length" /></label><button class="primary" :disabled="working || firstChapter < 1 || lastChapter < firstChapter || lastChapter > catalog.chapters.length" @click="startDownload">开始下载</button></div>
            <ol class="catalog-list"><li v-for="chapter in catalog.chapters.slice(0, 100)" :key="chapter.id"><span>{{ chapter.title }}</span><small>{{ chapter.restricted ? '网页受限' : '网页标记开放' }}</small></li></ol>
            <p v-if="catalog.chapters.length > 100" class="footnote">预览前 100 章；下载范围可选择完整目录。</p>
          </section>
        </template>

        <template v-if="page === 'library'">
          <section class="stats" aria-label="书库统计">
            <div><span>本地作品</span><strong>{{ data.books.length }}<small>本</small></strong></div>
            <div><span>已保存章节</span><strong>{{ saved }}<small>章</small></strong></div>
            <div><span>运行中任务</span><strong>{{ active }}<small>项</small></strong></div>
          </section>
          <div class="section-heading"><h2>我的书架</h2><input v-model="filter" type="search" placeholder="搜索书名或作者" aria-label="搜索书库" /></div>
          <section v-if="books.length" class="book-grid" aria-label="书籍列表">
            <article v-for="book in books" :key="book.id" class="book-card">
              <div class="book-cover"><span>LOCAL COLLECTION</span><h3>{{ book.title }}</h3><p>{{ book.author || '未标注作者' }}</p><i aria-hidden="true"></i></div>
              <div class="book-info">
<span class="source-tag">{{ sourceLabel(book.source) }}</span><h3>{{ book.title }}</h3><p>{{ book.saved_chapters }} / {{ book.total_chapters }} 章<span>{{ book.saved_chapters === book.total_chapters ? '完整保存' : '部分保存' }}</span></p>
                <div class="book-actions"><button :disabled="working || !book.saved_chapters" @click="saveBook(book.id, 'txt')">导出 TXT</button><button :disabled="working || !book.saved_chapters" @click="saveBook(book.id, 'epub')">导出 EPUB</button></div>
                <button v-if="book.source === 'fanqie-web'" class="update-book" :disabled="working || data.jobs.some(job => job.book_id === book.id && ['queued','running','paused'].includes(job.state))" @click="checkUpdates(book.id)">检查更新并补齐</button>
              </div>
            </article>
          </section>
          <div v-else class="empty-state"><span class="empty-icon" aria-hidden="true">▤</span><h2>{{ filter ? '没有找到这本书' : '从第一本书开始' }}</h2><p>{{ filter ? '试试其他书名或作者。' : '导入你持有的 JSON 内容，建立可以离线使用的书库。' }}</p><p v-if="!filter" class="muted">文件包含作品 ID、书名、作者及章节。格式见项目 examples/local-book.json。</p></div>
        </template>

        <template v-if="page === 'tasks'">
          <div class="section-heading"><h2>持久任务 <span>{{ data.jobs.length }}</span></h2><button class="subtle" :disabled="!desktopAvailable" @click="refresh">刷新状态</button></div>
          <div v-if="!data.jobs.length" class="empty-state"><span class="empty-icon" aria-hidden="true">↓</span><h2>还没有任务</h2><p>添加小说链接或导入本地内容，任务进度会保存在书库中。</p></div>
          <article v-for="job in data.jobs" :key="job.id" class="task-card">
            <div class="task-heading"><h3>{{ job.title }}</h3><span class="state-tag">{{ labels[job.state] || job.state }}</span></div>
            <progress :value="job.saved" :max="job.total" :aria-label="`${job.title}保存进度`"></progress>
            <div class="task-bottom"><span>本次范围已保存 {{ job.saved }} / {{ job.total }} 章 · 受限 {{ job.blocked }} · 失败 {{ job.failed }}</span><button v-if="job.state === 'running' || job.state === 'queued'" :disabled="working" @click="action(() => cancelJob(job.id))">取消任务</button><button v-else-if="job.state !== 'completed' && job.state !== 'blocked' && (job.state !== 'partial' || job.failed || job.saved + job.blocked < job.total)" :disabled="working" @click="action(() => resumeJob(job.id))">继续 / 重试</button></div>
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
        </template>

        <template v-if="page === 'sources'">
          <div class="source-intro"><h2>当前支持范围</h2><p>第一版面向免费平台。番茄官网网页接入仍处于实验阶段，网页受限章节会阻止整本完成；尚未达到第一版发布标准。</p></div>
          <div class="source-list">
            <article><span class="source-monogram">本</span><div><h3>本地 JSON 内容</h3><p>用户提供的作品与章节 · 任务恢复 · TXT / EPUB</p></div><span class="available">已接入</span></article>
            <article><span class="source-monogram">番</span><div><h3>番茄小说官网网页</h3><p>目录 · 网页开放正文 · 字体还原 · 范围下载 · 恢复与导出</p></div><span class="available">实验性</span></article>
            <article v-for="source in [{ name: '七猫小说 / 书旗小说', stage: '免费平台后续候选' }, { name: '起点中文网 / 晋江文学城', stage: '后续版本：目录、免费试读、账户与已购内容可行性研究' }]" :key="source.name"><span class="source-monogram pending">{{ source.name.slice(0, 1) }}</span><div><h3>{{ source.name }}</h3><p>{{ source.stage }}</p></div><span class="pending-label">未接入</span></article>
          </div>
          <p class="footnote">来源能力按目录、正文、账户与访问限制分别记录。付费平台留在后续路线，目前不承诺未购 VIP 正文获取能力。</p>
        </template>
        <footer>OpenNovel Downloader<span>开放的工具，长久的书库。</span></footer>
      </div>
    </main>
  </div>
</template>
