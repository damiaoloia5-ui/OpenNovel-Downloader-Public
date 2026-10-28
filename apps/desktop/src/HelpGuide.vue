<script setup lang="ts">
import {ref} from 'vue';
const emit=defineEmits<{navigate:[page:'download'|'accounts'|'library'|'tasks'];import:[]}>();
const topic=ref<'web'|'qidian'|'local'>('web');
</script>
<template>
  <section class="help-guide" aria-label="使用指引">
    <h2>你想从哪里开始？</h2>
    <div class="guide-tabs" role="group" aria-label="选择操作指引"><button :aria-pressed="topic==='web'" @click="topic='web'">免费网页小说</button><button :aria-pressed="topic==='qidian'" @click="topic='qidian'">起点账户内容</button><button :aria-pressed="topic==='local'" @click="topic='local'">本地文件</button></div>
    <template v-if="topic==='web'"><h3>粘贴链接，保存，再导出</h3><ol><li>进入“添加小说”，粘贴番茄、七猫或晋江的官网作品链接，点击“下一步”。</li><li>核对书名与当前获取范围，点击“仅保存目录标记开放章”。想先试用，可在“选章设置”里填 1 到 3，再点击“保存自定义范围”。</li><li>创建任务后直接进入本书工作台，在“保存正文”查看进度、暂停或继续。受限章保留缺章状态。</li><li>在工作台切到“导出文件”，选择 TXT 或 EPUB，点击“导出文件”，再点“打开文件所在文件夹”。也可从书架点击“打开作品工作台”。</li></ol><button class="primary" @click="emit('navigate','download')">去添加小说</button><p>书旗目前只有目录，不能保存正文；免费平台也可能只有部分网页章节开放。</p></template>
    <template v-else-if="topic==='qidian'"><h3>确认目录后，在同一本书的工作台继续</h3><ol><li>粘贴起点官网作品链接，会自动进入“起点保存引导”。也可直接进入“平台账户”。</li><li>点“打开官网登录”。首次会自动建立本地账户记录，登录请在官网亲自完成。回到软件后打开作品目录。</li><li>点击“读取作品目录”，核对书名和章数，再点“确认这本书”。确认后会自动进入本书工作台。已有作品可从书架直接打开，不必重复读取目录。</li><li>在工作台的“保存正文”中，本人先关闭官网自动订阅，再勾选确认项，点击“自动保存全书”。保持官网窗口打开，不要在运行时切页。</li><li>至少保存一章后，在同一工作台的“阅读”查看正文，或切到“导出文件”生成 TXT / EPUB。</li></ol><button class="primary" @click="emit('navigate','accounts')">打开起点保存引导</button><p>官网窗口打开不代表登录成功。遇到验证、订阅或不兼容页面会停止；本人处理后继续原任务。真实 VIP 全书仍待验收。</p></template>
    <template v-else><h3>导入已有文件，再离线阅读</h3><ol><li>点击“选择本地 JSON 文件”，选择符合 OpenNovel 格式、大小不超过 16 MiB 的文件。</li><li>导入后自动进入本书工作台，可在“保存正文”查看进度，在“阅读”和“导出文件”继续操作。</li></ol><button class="primary" @click="emit('import')">选择本地 JSON 文件</button><p>目前只导入 JSON，不直接导入 TXT、EPUB，也不能把作品链接当成文件。普通平台下载请使用“添加小说”。</p></template>
    <h3>常见问题</h3>
    <details><summary>没订阅能保存多少？为什么缺章？</summary><p>添加作品后，“当前获取范围”会区分目录标记开放、目录受限和正文待验证。点“仅保存目录标记开放章”会选择全部标记开放章，不受高级选章设置影响。实际保存可能仍遇到访问限制；工作台概览和保存正文页会汇总缺章原因，并可查看对应任务。登录、订阅、锁定、渠道限制不能仅凭一次失败判断，需本人核对官网。起点目录不包含阅读权益证明。</p></details>
    <details><summary>目录显示“未保存”，怎么办？</summary><p>目录只有章节名称，还没有正文。在本书工作台切到“保存正文”：起点使用账户官网窗口，其他已支持网页来源点击“检查目录并保存缺章”。</p><button @click="emit('navigate','accounts')">去起点保存引导</button></details>
    <details><summary>保存正文和导出文件有什么区别？</summary><p>保存正文是把内容放进软件书库，之后可以离线阅读。导出则会生成单独的 TXT 或 EPUB 文件，需要另外点“导出文件”。</p><button @click="emit('navigate','library')">打开我的书库</button></details>
    <details><summary>任务停止了，会丢失已保存章节吗？</summary><p>不会。先看停止原因，网络问题可稍后继续，登录或验证请本人处理。起点需在原账户窗口继续；普通来源在“任务进度”继续。软件不会自动订阅或购买。</p><button @click="emit('navigate','tasks')">查看任务进度</button></details>
    <details><summary>导出的文件在哪里？提示文件已存在怎么办？</summary><p>导出成功会显示完整路径和“打开文件所在文件夹”按钮。工作台的“导出文件”页也保留导出记录；同名旧文件不会覆盖，可以直接打开已有导出。</p></details>
    <details><summary>关闭软件后，起点需要重新登录吗？</summary><p>需要。账户官网会话为临时隔离会话，关闭后不保留登录凭据。书库和任务进度会保留，重新打开官网、自行登录后继续即可。</p></details>
  </section>
</template>
<style scoped>
.help-guide{max-width:800px;background:#fffefa;padding:26px;border:1px solid #dfe4d7;border-radius:8px}.guide-tabs{display:flex;flex-wrap:wrap;gap:10px;margin:20px 0}.guide-tabs [aria-pressed=true]{background:#354d32;color:#fff}.help-guide h3{font-size:18px;margin-top:28px}.help-guide li{padding:5px 0;line-height:1.9;font-size:14px}.help-guide p{font-size:13px;color:#526048;line-height:1.9;max-width:72ch}.help-guide details{padding:15px 0;border-top:1px solid #dfe4d7}.help-guide summary{cursor:pointer;font-size:14px;font-weight:600}
</style>
