<script setup lang="ts">
import {computed,nextTick,onMounted,ref,watch} from 'vue';
import type {ChapterContent} from './api';
import {readingFontSize,readingParagraphs} from './novelTypography';
const props=defineProps<{chapter:ChapterContent;bookTitle?:string}>();
const paragraphs=computed(()=>readingParagraphs(props.chapter.paragraphs));
const fontSize=ref(20);const paper=ref<HTMLElement|null>(null);
const key='opennovel.reader.font-size';
onMounted(()=>{try{fontSize.value=readingFontSize(localStorage.getItem(key));}catch{/* Reading works even if browser storage is unavailable. */}});
function resize(delta:number){fontSize.value=Math.min(28,Math.max(16,fontSize.value+delta));try{localStorage.setItem(key,String(fontSize.value));}catch{/* Optional preference only. */}}
watch(()=>props.chapter,async()=>{await nextTick();if(paper.value)paper.value.scrollTop=0;});
</script>
<template>
  <section class="novel-reader" aria-label="小说阅读">
    <div class="novel-toolbar"><span>阅读排版</span><div role="group" aria-label="阅读字号"><button type="button" :disabled="fontSize<=16" aria-label="缩小字号" @click="resize(-2)">A−</button><output aria-live="polite">{{ fontSize }} px</output><button type="button" :disabled="fontSize>=28" aria-label="放大字号" @click="resize(2)">A＋</button></div></div>
    <article ref="paper" class="novel-paper" tabindex="0" aria-label="已保存正文" :style="{'--reading-font-size':fontSize+'px'}">
      <header class="novel-heading"><div v-if="bookTitle" class="novel-book-title">{{ bookTitle }}</div><h3>{{ chapter.title }}</h3><span>已保存正文 · {{ paragraphs.length }} 段</span></header>
      <div class="novel-body"><p v-for="(paragraph,index) in paragraphs" :key="index">{{ paragraph }}</p></div>
      <footer v-if="$slots.default" class="novel-navigation"><slot /></footer>
    </article>
  </section>
</template>
<style scoped>
.novel-reader{min-width:0;border:1px solid #e4ddcf;border-radius:10px;background:#fcf9f2;overflow:hidden}.novel-toolbar{display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:12px;padding:12px 20px;border-bottom:1px solid #e4ddcf;font-size:12px;color:#756e60;background:#f7f3ea}.novel-toolbar>div{display:flex;gap:12px;align-items:center}.novel-toolbar button{padding:6px 12px;min-width:44px;min-height:36px;background:#fffdf8;color:#423c32;border:1px solid #dcd4c4;font-size:14px}.novel-toolbar output{font-variant-numeric:tabular-nums;min-width:38px;text-align:center}.novel-paper{max-height:75vh;min-height:360px;overflow-y:auto;overscroll-behavior:contain;scrollbar-gutter:stable;padding:36px clamp(20px,4vw,56px) 48px;color:#302b23}.novel-paper:focus-visible{outline:2px solid var(--accent);outline-offset:-3px}.novel-heading,.novel-body,.novel-navigation{max-width:720px;margin-inline:auto}.novel-heading{padding-bottom:26px;margin-bottom:26px;border-bottom:1px solid #e7e0d2}.novel-book-title{font-size:13px;color:#827969;line-height:1.8;margin-bottom:10px}.novel-heading h3{font-size:clamp(22px,2.2vw,28px);line-height:1.6;margin:0 0 14px;color:#302b23;font-weight:600;overflow-wrap:anywhere}.novel-heading>span{font-size:12px;color:#827969}.novel-body{font-family:"Noto Serif SC","Source Han Serif SC","Songti SC",SimSun,serif;font-size:var(--reading-font-size);font-weight:400;line-height:2;letter-spacing:.025em}.novel-body p{font:inherit;line-height:inherit;color:inherit;text-indent:2em;margin:0 0 1.1em;white-space:pre-wrap;overflow-wrap:anywhere;text-align:justify}.novel-navigation{border-top:1px solid #e4ddcf;margin-top:36px;padding-top:16px}@media(max-width:600px){.novel-paper{padding:24px 18px 32px;max-height:70vh}.novel-toolbar{padding:10px 14px}.novel-body{letter-spacing:0}.novel-heading{margin-bottom:20px;padding-bottom:20px}}
</style>
