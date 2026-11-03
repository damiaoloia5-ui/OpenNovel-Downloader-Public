<script setup lang="ts">
import {computed} from 'vue';
import {resolveSaveRange,type SaveOptions} from './saveOptions';
const props=defineProps<{modelValue:SaveOptions;total?:number;disabled?:boolean;hideFormat?:boolean}>();
const emit=defineEmits<{'update:modelValue':[value:SaveOptions]}>();
function update<K extends keyof SaveOptions>(key:K,value:SaveOptions[K]){emit('update:modelValue',{...props.modelValue,[key]:value});}
const error=computed(()=>{try{resolveSaveRange(props.modelValue,props.total);return '';}catch(e){return (e as Error).message;}});
</script>
<template>
  <fieldset class="save-options" :disabled="disabled">
<legend>保存范围与格式</legend>
    <div class="options-row">
<label>保存哪些章节？<select :value="modelValue.range" @change="update('range',($event.target as HTMLSelectElement).value as SaveOptions['range'])"><option value="all">全部章节</option><option value="range">指定起止章节</option></select></label>
    <label v-if="!hideFormat">保存格式<select :value="modelValue.format" @change="update('format',($event.target as HTMLSelectElement).value as SaveOptions['format'])"><option value="txt">TXT · 纯文本</option><option value="docx">Word · 已排版文档（.docx）</option></select></label>
</div>
    <div v-if="modelValue.range==='range'" class="options-row"><label>从第几章<input type="number" min="1" :max="total" step="1" :value="modelValue.first" @input="update('first',Number(($event.target as HTMLInputElement).value))"></label><label>到第几章<input type="number" min="1" :max="total" step="1" :value="modelValue.last" placeholder="填写结束序号" @input="update('last',($event.target as HTMLInputElement).value===''?null:Number(($event.target as HTMLInputElement).value))"></label></div>
    <p>按目录从 1 开始的顺序编号，包含起止两章{{ total?`；目录共 ${total} 章`:'' }}。指定范围只保存和导出这一段，已有正文会复用。</p>
    <p v-if="error" role="alert">{{ error }}</p><p v-if="!hideFormat&&modelValue.format==='docx'">Word：正文宋体／Times New Roman 小四，首行缩进 2 字符，1.5 倍行距；章标题黑体小三，作品信息与简介用楷体。</p>
  </fieldset>
</template>
<style scoped>
.save-options{min-width:0;margin:20px 0;padding:18px 20px;border:1px solid var(--border);border-radius:8px;background:var(--surface-soft)}legend{font-size:14px;font-weight:600;padding:0 6px}.options-row{display:flex;flex-wrap:wrap;gap:16px}.save-options label{display:grid;flex:1 1 190px;min-width:0;gap:8px;margin:8px 0;font-size:13px}.save-options input,.save-options select{box-sizing:border-box;min-width:0;width:100%;padding:10px;border:1px solid var(--border);border-radius:5px;background:var(--surface);color:var(--ink);font:inherit}.save-options p{font-size:12px;line-height:1.8;color:var(--muted);margin:10px 0 0}.save-options p[role=alert]{color:var(--danger)}
</style>
