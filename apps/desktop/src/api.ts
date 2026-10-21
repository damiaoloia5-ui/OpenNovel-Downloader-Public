import { invoke, isTauri } from '@tauri-apps/api/core';

export interface Book {
  id: string;
  title: string;
  author: string;
  source: string;
  remote_id: string;
  saved_chapters: number;
  total_chapters: number;
}

export interface Job {
  source: string;
  blocked: number;
  failed: number;
  id: string;
  book_id: string;
  title: string;
  state: string;
  saved: number;
  total: number;
  error: string | null;
}

export interface Snapshot { books: Book[]; jobs: Job[] }
export interface JobItem {chapter_id:string;position:number;title:string;state:string;error:string|null;error_kind:string|null;retryable:boolean}
export interface JobDetails {job:Job;items:JobItem[]}
export interface Catalog { source: string; remote_id: string; title: string; author: string; description:string; chapters: { id: string; title: string; restricted: boolean }[] }
export interface SourceInfo {id:string;name:string;can_download:boolean;can_search:boolean;note:string;example:string}
export interface SearchResult {source:string;remote_id:string;title:string;author:string;input:string}
export interface BatchItem {position:number;input:string;state:string;job_id:string|null;error:string|null}
export interface Batch {id:string;state:string;selection:string;duplicates:number;items:BatchItem[];error:string|null}
export function searchBooks(source:string,query:string):Promise<SearchResult[]> {return invoke('search_books',{source,query});}
export function batches():Promise<Batch[]> {return invoke('batch_snapshot');}
export function createBatch(inputs:string[],selection:string):Promise<string> {return invoke('create_batch',{inputs,selection});}
export function resumeBatch(id:string):Promise<void> {return invoke('resume_batch',{id});}
export function stopBatch(id:string,paused:boolean):Promise<void> {return invoke('stop_batch',{id,paused});}
export function sourceCapabilities():Promise<SourceInfo[]> {return invoke('source_capabilities');}
export interface ChapterContent {id:string;title:string;paragraphs:string[]}
export interface ExportRecord {id:string;book_id:string;job_id:string|null;path:string;format:string;saved:number;total:number;partial:boolean;created_at:string}
export interface BookDetails {book:Book;description:string;acquired_at:string;chapters:{id:string;title:string;position:number;saved:boolean}[];exports:ExportRecord[]}
export interface Receipt { path: string; saved_chapters: number; total_chapters: number; partial: boolean }
export interface BookPreferences {book_id:string;tags:string[];archived:boolean}
export interface MaintenanceReceipt {id:string;path:string;books:number;chapters:number}
export interface DiagnosticReport {version:string;operating_system:string;architecture:string;schema:number;books:number;saved_chapters:number;jobs:number;job_states:Record<string,number>;integrity_ok:boolean;foreign_keys_ok:boolean}
export function preferences():Promise<BookPreferences[]> {return invoke('book_preferences');}
export function organizeBook(bookId:string,tags:string[],archived:boolean):Promise<BookPreferences> {return invoke('organize_book',{bookId,tags,archived});}
export function backupLibrary():Promise<MaintenanceReceipt> {return invoke('backup_library');}
export function restoreLibrary(backupPath:string):Promise<MaintenanceReceipt> {return invoke('restore_library',{backupPath});}
export function openRestoredLibrary(id:string):Promise<void> {return invoke('open_restored_library',{id});}
export function previewDiagnostics():Promise<DiagnosticReport> {return invoke('preview_diagnostics');}
export function saveDiagnostics():Promise<string> {return invoke('save_diagnostics');}

export const desktopAvailable = isTauri();

export function snapshot(): Promise<Snapshot> { return invoke('library_snapshot'); }
export function inspectBook(input: string): Promise<Catalog> { return invoke('inspect_book', { input }); }
export function downloadBook(input: string, first: number, last: number,selection?:string): Promise<string> { return invoke('download_book', { input, first, last,selection:selection || null }); }
export function bookDetails(bookId:string):Promise<BookDetails> {return invoke('book_details',{bookId});}
export function readChapter(bookId:string,chapterId:string):Promise<ChapterContent> {return invoke('read_chapter',{bookId,chapterId});}
export function exportJob(jobId:string,format:'txt'|'epub'):Promise<Receipt> {return invoke('export_job',{jobId,format});}
export function revealExport(id:string):Promise<void> {return invoke('reveal_export',{id});}
export function pauseJob(jobId:string):Promise<void> {return invoke('pause_job',{jobId});}
export function importBook(json: string): Promise<string> { return invoke('import_book', { json }); }
export function resumeJob(jobId: string): Promise<void> { return invoke('resume_job', { jobId }); }
export function jobDetails(jobId:string):Promise<JobDetails> {return invoke('job_details',{jobId});}
export function retryChapters(jobId:string,chapters:string[]):Promise<void> {return invoke('retry_chapters',{jobId,chapters});}
export function updateBook(bookId:string):Promise<string> {return invoke('update_book',{bookId});}
export function cancelJob(jobId: string): Promise<void> { return invoke('cancel_job', { jobId }); }
export function exportBook(bookId: string, format: 'txt' | 'epub'): Promise<Receipt> {
  return invoke('export_book', { bookId, format });
}
