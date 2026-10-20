<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { cancelJob, desktopAvailable, exportBook, importBook, resumeJob, snapshot, type Snapshot } from './api';

const page = ref<'library' | 'tasks' | 'sources'>('library');
const data = ref<Snapshot>({ books: [], jobs: [] });
const error = ref('');
const notice = ref('');
const working = ref(false);
const filter = ref('');
const fileInput = ref<HTMLInputElement | null>(null);
const books = computed(() => data.value.books.filter(book => `${book.title} ${book.author}`.toLocaleLowerCase().includes(filter.value.toLocaleLowerCase())));
const saved = computed(() => data.value.books.reduce((total, book) => total + book.saved_chapters, 0));
const active = computed(() => data.value.jobs.filter(job => job.state === 'queued' || job.state === 'running').length);
const labels: Record<string, string> = { queued: '等待导入', running: '正在保存', paused: '中断待继续', cancelled: '已取消', failed: '导入失败', completed: '已完成' };
let timer: ReturnType<typeof setInterval> | undefined;
let refreshing = false;

async function refresh() {
  if (!desktopAvailable || refreshing) return;
  refreshing = true;
  try { data.value = await snapshot(); }
  catch (reason) { error.value = String(reason); }
  finally { refreshing = false; }
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
        <button :class="{ selected: page === 'library' }" @click="page = 'library'"><span aria-hidden="true">▤</span>我的书库<b>{{ data.books.length }}</b></button>
        <button :class="{ selected: page === 'tasks' }" @click="page = 'tasks'"><span aria-hidden="true">↓</span>导入任务<b v-if="active">{{ active }}</b></button>
        <button :class="{ selected: page === 'sources' }" @click="page = 'sources'"><span aria-hidden="true">◎</span>小说来源</button>
      </nav>
      <div class="sidebar-note"><span class="dot"></span>本地保存，离线导出<p>工程预览 · 0.0.1</p></div>
    </aside>

    <main>
      <header class="topbar"><span>个人工作空间 / {{ page === 'library' ? '书库' : page === 'tasks' ? '任务' : '来源' }}</span><span class="version">开发预览</span></header>
      <div class="main-content">
        <div v-if="!desktopAvailable" class="message warning" role="status">当前是浏览器界面预览。请运行桌面应用以使用本地书库和导出功能。</div>
        <div v-if="error" class="message failure" role="alert">{{ error }}<button aria-label="关闭错误提示" @click="error = ''">×</button></div>
        <div v-if="notice" class="message success" role="status">{{ notice }}<button aria-label="关闭提示" @click="notice = ''">×</button></div>
        <div class="page-heading">
          <div><p class="eyebrow">YOUR STORIES, KEPT CLOSE</p><h1>{{ page === 'library' ? '让故事留在身边。' : page === 'tasks' ? '每一章，都有着落。' : '一个书库，更多来源。' }}</h1><p class="intro">{{ page === 'library' ? '整理已经拥有的文字，随时导出、再次阅读。' : page === 'tasks' ? '查看真实保存进度，继续中断的任务。' : '平台能力逐项验证，支持范围持续公开。' }}</p></div>
          <button v-if="page !== 'sources'" class="primary" :disabled="!desktopAvailable || working" @click="fileInput?.click()">＋ 导入本地书籍</button>
          <input ref="fileInput" type="file" accept=".json,application/json" class="file-input" aria-label="选择 JSON 书籍文件" @change="chooseFile" />
        </div>

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
<span class="source-tag">本地 JSON</span><h3>{{ book.title }}</h3><p>{{ book.saved_chapters }} / {{ book.total_chapters }} 章<span>{{ book.saved_chapters === book.total_chapters ? '完整保存' : '部分保存' }}</span></p>
                <div class="book-actions"><button :disabled="working || !book.saved_chapters" @click="saveBook(book.id, 'txt')">导出 TXT</button><button :disabled="working || !book.saved_chapters" @click="saveBook(book.id, 'epub')">导出 EPUB</button></div>
              </div>
            </article>
          </section>
          <div v-else class="empty-state"><span class="empty-icon" aria-hidden="true">▤</span><h2>{{ filter ? '没有找到这本书' : '从第一本书开始' }}</h2><p>{{ filter ? '试试其他书名或作者。' : '导入你持有的 JSON 内容，建立可以离线使用的书库。' }}</p><p v-if="!filter" class="muted">文件包含作品 ID、书名、作者及章节。格式见项目 examples/local-book.json。</p></div>
        </template>

        <template v-if="page === 'tasks'">
          <div class="section-heading"><h2>持久任务 <span>{{ data.jobs.length }}</span></h2><button class="subtle" :disabled="!desktopAvailable" @click="refresh">刷新状态</button></div>
          <div v-if="!data.jobs.length" class="empty-state"><span class="empty-icon" aria-hidden="true">↓</span><h2>还没有导入任务</h2><p>选择本地书籍文件，任务进度会保存在书库中。</p></div>
          <article v-for="job in data.jobs" :key="job.id" class="task-card">
            <div class="task-heading"><h3>{{ job.title }}</h3><span class="state-tag">{{ labels[job.state] || job.state }}</span></div>
            <progress :value="job.saved" :max="job.total" :aria-label="`${job.title}保存进度`"></progress>
            <div class="task-bottom"><span>已保存 {{ job.saved }} / {{ job.total }} 章</span><button v-if="job.state === 'running' || job.state === 'queued'" :disabled="working" @click="action(() => cancelJob(job.id))">取消任务</button><button v-else-if="job.state !== 'completed'" :disabled="working" @click="action(() => resumeJob(job.id))">继续导入</button></div>
            <p v-if="job.error" class="task-error">{{ job.error }}</p>
          </article>
        </template>

        <template v-if="page === 'sources'">
          <div class="source-intro"><h2>当前支持范围</h2><p>本轮完成本地导入与任务引擎。商业来源尚未接入，不能用于网络小说下载。</p></div>
          <div class="source-list">
            <article><span class="source-monogram">本</span><div><h3>本地 JSON 内容</h3><p>用户提供的作品与章节 · 任务恢复 · TXT / EPUB</p></div><span class="available">已接入</span></article>
            <article v-for="source in [{ name: '番茄小说', stage: '首来源预研' }, { name: '七猫小说 / 书旗小说', stage: '第二来源候选' }, { name: '起点中文网 / 晋江文学城', stage: '账户与付费场景研究' }]" :key="source.name"><span class="source-monogram pending">{{ source.name.slice(0, 1) }}</span><div><h3>{{ source.name }}</h3><p>{{ source.stage }}</p></div><span class="pending-label">未接入</span></article>
          </div>
          <p class="footnote">平台支持将分别记录目录、正文、账户和访问限制；未购买 VIP 内容的获取能力尚未验证。</p>
        </template>
        <footer>OpenNovel Downloader<span>开放的工具，长久的书库。</span></footer>
      </div>
    </main>
  </div>
</template>
