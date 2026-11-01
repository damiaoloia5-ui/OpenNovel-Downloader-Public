<script setup lang="ts">
withDefaults(defineProps<{name?:string;size?:number}>(),{name:'book',size:20});
const paths:Record<string,string>={
  book:'M12 5.5C9.3 3.8 6.1 3.4 3 4v15c3.1-.6 6.3-.2 9 1.5M12 5.5c2.7-1.7 5.9-2.1 9-1.5v15c-3.1-.6-6.3-.2-9 1.5M12 5.5v15',
  plus:'M12 5v14M5 12h14',
  library:'M4 4h4v16H4zM10 4h4v16h-4zM16 5l3-1 4 15-3 1z',
  download:'M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5',
  help:'M9.1 9a3 3 0 1 1 5.7 1.3C14 11.4 12 12 12 14M12 17h.01M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0',
  globe:'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0M3 12h18M12 3c-4 4-4 14 0 18 4-4 4-14 0-18',
  user:'M16 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0M4 21v-2a8 8 0 0 1 16 0v2',
  archive:'M3 3h18v5H3zM5 8v13h14V8M9 12h6',
  link:'M10 13a5 5 0 0 0 7 .2l3-3a5 5 0 0 0-7-7l-2 2M14 11a5 5 0 0 0-7-.2l-3 3a5 5 0 0 0 7 7l2-2',
  file:'M14 2H5v20h14V7zM14 2v5h5M8 12h8M8 16h6',
  folder:'M3 5h6l2 3h10v12H3z',
  chevron:'m9 5 7 7-7 7',
};
</script>
<template>
  <svg class="app-icon" :width="size" :height="size" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path :d="paths[name]||paths.book" /></svg>
</template>
