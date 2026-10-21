<script setup lang="ts">
import {computed,ref} from 'vue';
import {searchBooks,desktopAvailable,type SourceInfo,type SearchResult} from './api';
const props=defineProps<{sources:SourceInfo[];busy:boolean}>();
const emit=defineEmits<{select:[input:string]}>();
const available=computed(()=>props.sources.filter(s=>s.can_search));
const source=ref('jinjiang-web');const query=ref('');const results=ref<SearchResult[]>([]);const error=ref('');const working=ref(false);const searched=ref(false);
async function search(){if(working.value||props.busy)return;error.value='';working.value=true;searched.value=false;results.value=[];try{results.value=await searchBooks(source.value,query.value);searched.value=true;}catch(e){error.value=String(e);}finally{working.value=false;}}
</script>
<template>
  <details class="download-panel">
    <summary>站内搜索</summary>
    <p class="footnote">仅列出已验证的来源和首屏最多 20 项。搜索结果保留来源身份，读取目录后再选择下载。</p>
    <form @submit.prevent="search"><label>搜索来源<select v-model="source" :disabled="working||busy"><option v-for="s in available" :key="s.id" :value="s.id">{{ s.name }}</option></select></label><div class="link-row"><input v-model="query" aria-label="来源搜索词" placeholder="作品标题，最多 80 字" maxlength="80" :disabled="working||busy"><button :disabled="!desktopAvailable||working||busy||!query.trim()||!available.length">搜索作品</button></div></form>
    <p v-if="error" class="message failure" role="alert">{{ error }}</p>
    <p v-if="searched&&!results.length" class="footnote">本页未找到作品，或请更换关键词。</p>
    <ul v-if="results.length" class="results" aria-label="来源搜索结果"><li v-for="r in results" :key="`${r.source}:${r.remote_id}`"><div><strong>{{ r.title }}</strong><small>{{ r.author||'未标注作者' }} · {{ sources.find(s=>s.id===r.source)?.name }} · {{ r.remote_id }}</small></div><button :disabled="working||busy" @click="emit('select',r.input)">读取此书目录</button></li></ul>
  </details>
</template>
<style scoped>
summary{cursor:pointer;font-size:16px;font-weight:600}label{font-size:13px}select{margin:0 0 12px 12px;padding:6px;max-width:100%}.results{list-style:none;padding:0}.results li{display:flex;gap:16px;justify-content:space-between;align-items:center;padding:12px 0;border-bottom:1px solid #e4e8dd}.results small{display:block;color:#748069;margin-top:5px}.results button{flex-shrink:0}
</style>
