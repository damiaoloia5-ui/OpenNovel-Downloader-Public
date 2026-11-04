<script setup lang="ts">
import {onMounted,onUnmounted,ref} from 'vue';
import {chooseSaveDirectory,defaultExportRoot,desktopAvailable} from './api';
const props=defineProps<{modelValue:string|null;disabled?:boolean}>();
const emit=defineEmits<{'update:modelValue':[directory:string|null];busy:[value:boolean]}>();
const root=ref('应用目录 / exports');const picking=ref(false);const error=ref('');let disposed=false;
onMounted(async()=>{if(desktopAvailable)try{const value=await defaultExportRoot();if(!disposed)root.value=value;}catch(e){if(!disposed)error.value=String(e);}});
onUnmounted(()=>{disposed=true;emit('busy',false);});
async function choose(){if(props.disabled||picking.value)return;picking.value=true;error.value='';emit('busy',true);try{const value=await chooseSaveDirectory(props.modelValue);if(!disposed&&value!==null)emit('update:modelValue',value);}catch(e){if(!disposed)error.value=String(e);}finally{picking.value=false;emit('busy',false);}}
</script>
<template>
  <section class="save-location" aria-label="文件保存位置">
    <div class="location-heading"><strong>文件保存到哪里？</strong><span>{{ modelValue?'自选文件夹':'默认位置' }}</span></div>
    <output class="location-path" aria-live="polite">{{ modelValue||root }}</output>
    <div class="location-actions"><button type="button" :disabled="disabled||picking||!desktopAvailable" @click="choose">{{ picking?'请选择文件夹…':'选择文件夹' }}</button><button v-if="modelValue" type="button" :disabled="disabled||picking" @click="emit('update:modelValue',null)">使用默认位置</button></div>
    <p class="location-note">{{ modelValue?'文件会直接保存到所选文件夹；选择公共位置时请注意文件共享。':'默认放在当前用户数据目录的 exports 下，每本书一个子文件夹，与其他用户分开。' }}文件名：平台－书名－作者（扩展名随格式变化）。选好位置后再开始保存。</p>
    <p v-if="error" class="location-error" role="alert">{{ error }}</p>
  </section>
</template>
<style scoped>
.save-location{margin:20px 0;padding:18px 20px;border:1px solid var(--border);border-radius:8px;background:var(--surface-soft);min-width:0}.location-heading{display:flex;align-items:center;flex-wrap:wrap;gap:12px;font-size:14px}.location-heading span{color:var(--muted);font-size:12px}.location-path{display:block;margin:12px 0;overflow-wrap:anywhere;font-size:14px;line-height:1.7;color:var(--ink)}.location-actions{display:flex;flex-wrap:wrap;gap:10px}.save-location .location-note{font-size:12px;color:var(--muted);line-height:1.8;margin:12px 0 0}.save-location .location-error{color:var(--danger);font-size:13px;margin:12px 0 0}@media(max-width:600px){.save-location{padding:14px}}
</style>
