<script setup lang="ts">
import {ref} from 'vue';
const emit=defineEmits<{navigate:[page:'download'|'accounts'|'library'|'tasks'];import:[]}>();
const topic=ref<'web'|'qidian'|'local'>('web');
</script>
<template>
  <section class="help-guide" aria-label="使用指引">
    <h2>只做三件事：粘贴链接、按提示操作、拿到 TXT</h2><p>不需要选择平台、创建账号记录或理解任务。默认保存全书中能获取的正文，已保存章节会自动跳过。</p>
    <div class="guide-tabs" role="group" aria-label="选择操作指引"><button :aria-pressed="topic==='web'" @click="topic='web'">免费网页小说</button><button :aria-pressed="topic==='qidian'" @click="topic='qidian'">起点小说</button><button :aria-pressed="topic==='local'" @click="topic='local'">已有本地文件</button></div>
    <template v-if="topic==='web'"><h3>免费网页小说</h3><ol><li>在浏览器打开小说的作品介绍页，复制顶部地址。</li><li>进入“添加小说”，粘贴地址，点“开始保存为 TXT”。软件会识别平台并开始保存。</li><li>保存结束后，点“打开 TXT 所在文件夹”，里面就是文本文件。也可在“我的书库”直接打开 TXT 文件夹。</li></ol><button class="primary" @click="emit('navigate','download')">去粘贴小说链接</button><p>支持范围以实际官网开放内容为准；书旗目前只有目录，不能保存正文。文件会标明已保存章数，有缺章时不会假装完整。</p></template>
    <template v-else-if="topic==='qidian'"><h3>起点小说</h3><ol><li>在“添加小说”粘贴起点作品链接，点“开始保存为 TXT”。</li><li>点“打开起点，去登录”，在官网完成登录，回到软件点“我已登录，下一步”。软件会自动打开并尝试读取目录。</li><li>核对书名和章数，在官网关闭自动订阅后勾选确认项，点“是这本，开始保存为 TXT”。</li><li>保持官网窗口打开，不要手动翻页。结束后点“打开 TXT 所在文件夹”。已经保存的书直接从“我的书库”继续，不用重新添加。</li></ol><button class="primary" @click="emit('navigate','download')">去粘贴起点链接</button><p>如果提示目录不完整，在官网展开全部章节后点“我已处理，重新读取”。遇到验证码先在官网完成；官网窗口打开不代表已经登录，真实已购全书仍待验收。</p></template>
    <template v-else><h3>已有本地文件</h3><ol><li>在“添加小说”展开“其他方式”，选择本地 JSON 文件。</li><li>导入后会自动生成 TXT，也可以在软件中阅读。</li></ol><button class="primary" @click="emit('import')">选择本地 JSON 文件</button><p>目前只接受 OpenNovel 格式的 JSON（不超过 16 MiB），暂不直接导入 TXT 或 EPUB。</p></template>
    <h3>卡住时，按这里处理</h3>
    <details><summary>番茄网页只保存了前几章，怎么办？</summary><p>在“添加小说”粘贴作品链接，点“开始保存为 TXT”，按提示首次启用本地增强。Windows 一体化版已内置所需组件，无需安装 Docker、Python 或开发工具；以后直接粘贴链接保存。增强只保存明确免费且通过校验的正文，来源受限时会保留进度并给出提示，不能保证每本书完整。结果与原网页版本分开。</p><button @click="emit('navigate','download')">去添加小说</button></details>
    <details><summary>保存停止了，我该点什么？</summary><p>页面会显示“下一步”。网络问题稍后点“继续保存”；验证码或登录问题先处理官网窗口，再回来继续。已保存正文不会丢失。想中途退出，先点“暂停保存”，下次从书库打开这本书继续。</p><button @click="emit('navigate','tasks')">查看保存进度</button></details>
    <details><summary>正文保存好了，为什么还没看到 TXT？</summary><p>等待本次保存任务结束或暂停，软件会在后台生成 TXT。页面显示“TXT 已生成”后，点“打开 TXT 所在文件夹”。若生成失败，点“重试生成 TXT”；已保存正文仍在书库。</p><button @click="emit('navigate','library')">打开我的书库</button></details>
    <details><summary>为什么只有目录或部分章节？</summary><p>目录只有章节名称。正文能保存多少取决于官网开放情况、实际阅读权限和页面是否受支持。打开这本书的“保存进度”，展开“为什么有些章节没保存？”可查看原因。软件不会自动订阅或购买。</p></details>
    <details><summary>想调速度、选章、导出 EPUB，在哪里？</summary><p>保存页展开“更多设置”可调整速度、选章或取消任务。想要 EPUB，在书籍页面选“其他格式与文件记录”。账号管理、来源说明和备份在左侧“更多功能”。默认设置即可保存，不必先调参数。</p></details>
    <details><summary>关闭软件后，需要重新登录吗？</summary><p>起点官网使用临时会话，关闭后需重新登录。小说正文、TXT 和保存进度会保留；从书库打开这本书，按提示登录并继续即可。</p></details>
  </section>
</template>
<style scoped>
.help-guide{max-width:800px;background:var(--surface);padding:26px;border:1px solid var(--border);border-radius:8px}.guide-tabs{display:flex;flex-wrap:wrap;gap:10px;margin:20px 0}.guide-tabs [aria-pressed=true]{background:var(--accent);color:#fff}.help-guide h3{font-size:18px;margin-top:28px}.help-guide li{padding:5px 0;line-height:1.9;font-size:14px}.help-guide p{font-size:13px;color:var(--muted);line-height:1.9;max-width:72ch}.help-guide details{padding:15px 0;border-top:1px solid var(--border)}.help-guide summary{cursor:pointer;font-size:14px;font-weight:600}
</style>
