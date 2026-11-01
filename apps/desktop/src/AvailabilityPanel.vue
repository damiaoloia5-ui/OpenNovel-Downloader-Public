<script setup lang="ts">
import {computed,ref,watch} from 'vue';
import type {ReadingChapter} from './api';
import {chapterEvidence,missingGroups,availabilityLabels} from './availabilityModel';
const props=defineProps<{chapters:ReadingChapter[];busy?:boolean}>();
const emit=defineEmits<{job:[id:string];save:[];read:[]}>();
const filter=ref('all');const search=ref('');const page=ref(1);
const groups=computed(()=>missingGroups(props.chapters));
const missing=computed(()=>props.chapters.filter(c=>!c.saved));
const filtered=computed(()=>missing.value.filter(c=>(filter.value==='all'||chapterEvidence(c).state===filter.value)&&`${c.position} ${c.title}`.includes(search.value)));
const pages=computed(()=>Math.max(1,Math.ceil(filtered.value.length/50)));
const visible=computed(()=>filtered.value.slice((page.value-1)*50,page.value*50));
watch([filter,search],()=>{page.value=1;});watch(pages,n=>{page.value=Math.min(page.value,n);});
</script>
<template>
  <section class="availability-panel" aria-label="缺章原因与下一步">
    <h2>缺章原因与下一步</h2>
    <p>本地已保存 {{ chapters.length-missing.length }}/{{ chapters.length }} 章，尚缺 {{ missing.length }} 章。原因来自最近确认目录及每章最近任务，不会重新请求官网。</p>
    <p v-if="!missing.length" role="status">当前目录正文已收齐，可以阅读或导出。</p>
    <template v-else>
      <ul class="reason-summary"><li v-for="group in groups" :key="group.state"><strong>{{ group.label }} {{ group.count }} 章</strong><p>{{ group.advice }}</p></li></ul>
      <div class="reason-actions"><button :disabled="busy" @click="emit('save')">去保存正文</button><button v-if="chapters.length>missing.length" :disabled="busy" @click="emit('read')">阅读已保存内容</button></div>
      <details>
<summary>查看全部缺失章节及原始原因</summary>
        <div class="reason-filters"><label>缺章类型<select v-model="filter"><option value="all">全部缺章</option><option v-for="group in groups" :key="group.state" :value="group.state">{{ group.label }}（{{ group.count }}）</option></select></label><label>查找章节<input v-model="search" type="search" placeholder="序号或标题" /></label></div>
        <ol class="reason-list"><li v-for="chapter in visible" :key="chapter.id"><strong>{{ chapter.position }} · {{ chapter.title }}</strong><span>{{ availabilityLabels[chapterEvidence(chapter).state]||'状态待核对' }}：{{ chapterEvidence(chapter).reason }}</span><button v-if="chapterEvidence(chapter).job_id" :disabled="busy" @click="emit('job',chapterEvidence(chapter).job_id!)">查看相关任务</button></li></ol>
        <p v-if="!filtered.length">没有符合条件的缺章。</p><div v-if="pages>1" class="reason-actions"><button :disabled="page<=1" @click="page--">上一页</button><span>{{ page }}/{{ pages }}</span><button :disabled="page>=pages" @click="page++">下一页</button></div>
      </details>
    </template>
  </section>
</template>
<style scoped>
.availability-panel{margin:24px 0;padding-top:18px;border-top:1px solid var(--border);min-width:0}.availability-panel h2{font-size:19px}.availability-panel p,.reason-list{font-size:13px;line-height:1.9;color:var(--muted)}.reason-summary,.reason-list{list-style:none;padding:0}.reason-summary li{padding:12px 0;border-bottom:1px solid var(--border)}.reason-summary p{margin:5px 0}.reason-actions,.reason-filters{display:flex;flex-wrap:wrap;gap:12px;align-items:center;margin:16px 0}.reason-filters label{display:grid;gap:8px;max-width:100%;font-size:13px}.reason-filters input,.reason-filters select{font:inherit;padding:10px;border:1px solid var(--border);border-radius:5px;min-width:0;max-width:100%;box-sizing:border-box}.reason-list li{display:grid;gap:8px;padding:14px 0;border-bottom:1px solid var(--border);overflow-wrap:anywhere}.reason-list button{justify-self:start}.availability-panel summary{cursor:pointer;font-size:13px;font-weight:600}
</style>
