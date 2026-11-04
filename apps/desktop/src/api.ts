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
export interface SourceInfo {id:string;name:string;can_inspect?:boolean;can_download:boolean;can_search:boolean;note:string;example:string}
export interface SearchResult {source:string;remote_id:string;title:string;author:string;input:string}
export interface BatchItem {position:number;input:string;state:string;job_id:string|null;error:string|null}
export interface Batch {id:string;state:string;selection:string;duplicates:number;items:BatchItem[];error:string|null}
export function searchBooks(source:string,query:string):Promise<SearchResult[]> {return invoke('search_books',{source,query});}
export function batches():Promise<Batch[]> {return invoke('batch_snapshot');}
export function createBatch(inputs:string[],selection:string):Promise<string> {return invoke('create_batch',{inputs,selection});}
export function resumeBatch(id:string):Promise<void> {return invoke('resume_batch',{id});}
export function stopBatch(id:string,paused:boolean):Promise<void> {return invoke('stop_batch',{id,paused});}
export interface CatalogChange {chapter_id:string;kind:string;previous_position:number|null;position:number|null;previous_title:string|null;title:string|null}
export interface CatalogUpdate {previous_hash:string;proposed_hash:string;changed:boolean;changes:CatalogChange[];metadata:{field:string;previous:string;proposed:string}[]}
export interface UpdatePreview {token:string;diff:CatalogUpdate}
export interface HistoryEntry {id:string;book_id:string;title:string;reason:string;created_at:string;saved:number;total:number}
export function previewCatalogUpdate(bookId:string):Promise<UpdatePreview> {return invoke('preview_catalog_update',{bookId});}
export function acceptCatalogUpdate(token:string):Promise<string> {return invoke('accept_catalog_update',{token});}
export function refreshBookChapter(bookId:string,chapterId:string):Promise<{changed:boolean;snapshot_id:string|null}> {return invoke('refresh_book_chapter',{bookId,chapterId});}
export function bookHistory(bookId:string):Promise<HistoryEntry[]> {return invoke('book_history',{bookId});}
export function historyDetails(id:string):Promise<BookDetails> {return invoke('history_details',{id});}
export function readHistory(id:string,chapterId:string):Promise<ChapterContent> {return invoke('read_history',{id,chapterId});}
export function exportHistory(id:string,format:'txt'|'epub'|'docx'):Promise<Receipt> {return invoke('export_history',{id,format});}
export function sourceCapabilities():Promise<SourceInfo[]> {return invoke('source_capabilities');}
export type LocalEngineIssue='component_unavailable'|'docker_missing'|'docker_unavailable'|'docker_windows_mode'|'images_missing'|'engine_missing'|'source_changed'|'offline_bundle_pending'|'offline_bundle_invalid'|'offline_import_failed'|'native_bundle_invalid'|'native_sandbox_failed'|'check_failed';
export interface LocalEngineStatus {enabled:boolean;ready:boolean;remembered?:boolean;installed?:boolean;message:string;issue?:LocalEngineIssue|null;next_step?:string}
export function localEngineStatus():Promise<LocalEngineStatus> {return invoke('local_engine_status');}
export function setLocalEngine(enabled:boolean):Promise<LocalEngineStatus> {return invoke('set_local_engine',{enabled});}
export interface ChapterContent {id:string;title:string;paragraphs:string[]}
export interface ExportRecord {id:string;book_id:string;job_id:string|null;path:string;format:string;saved:number;total:number;partial:boolean;created_at:string}
export interface ChapterAvailability {state:string;reason:string;job_id:string|null}
export interface ReadingChapter {id:string;title:string;position:number;saved:boolean;availability?:ChapterAvailability|null}
export interface BookDetails {book:Book;description:string;acquired_at:string;chapters:ReadingChapter[];exports:ExportRecord[]}
export interface Receipt { path: string; saved_chapters: number; total_chapters: number; partial: boolean }
export interface AutoTxtStatus {book_id:string;state:string;receipt:Receipt|null;export_id:string|null;error:string|null}
export function automaticTxtSnapshot():Promise<AutoTxtStatus[]> {return invoke('automatic_txt_snapshot');}
export interface SaveDestination {directory:string|null;format?:string;selection_only?:boolean}
export interface LocalProfile {id:string;name:string;database:string|null}
export interface ProfileOverview {current_id:string;profiles:LocalProfile[];data_directory:string}
export function localProfiles():Promise<ProfileOverview> {return invoke('local_profiles');}
export function createLocalProfile(name:string):Promise<LocalProfile> {return invoke('create_local_profile',{name});}
export function switchLocalProfile(id:string):Promise<void> {return invoke('switch_local_profile',{id});}
export function defaultExportRoot():Promise<string> {return invoke('default_export_root');}
export function chooseSaveDirectory(current:string|null):Promise<string|null> {return invoke('choose_save_directory',{current});}
export interface ExportDestination {directory:string;custom:boolean;filename:string;format?:string}
export function bookExportDestination(bookId:string):Promise<ExportDestination> {return invoke('book_export_destination',{bookId});}
export function chooseBookExportDirectory(bookId:string):Promise<ExportDestination|null> {return invoke('choose_book_export_directory',{bookId});}
export function setBookExportDirectory(bookId:string,directory:string|null):Promise<ExportDestination> {return invoke('set_book_export_directory',{bookId,directory});}
export function retryAutomaticTxt(bookId:string):Promise<void> {return invoke('retry_automatic_txt',{bookId});}
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

export interface SoftwareUpdateStatus {phase:string;current_version:string;version:string|null;notes:string;downloaded:number;total:number|null;message:string;backup_directory:string|null;portable:boolean}
export function softwareUpdateStatus():Promise<SoftwareUpdateStatus> {return invoke('software_update_status');}
export function checkSoftwareUpdate():Promise<SoftwareUpdateStatus> {return invoke('check_software_update');}
export function downloadSoftwareUpdate():Promise<SoftwareUpdateStatus> {return invoke('download_software_update');}
export function installSoftwareUpdate(convertPortable:boolean):Promise<void> {return invoke('install_software_update',{convertPortable});}
export function cancelSoftwareDownload():Promise<void> {return invoke('cancel_software_download');}
export function openSoftwareReleases():Promise<void> {return invoke('open_software_releases');}

export function snapshot(): Promise<Snapshot> { return invoke('library_snapshot'); }
export function snapshotIfChanged(known:string|null):Promise<{revision:string;snapshot:Snapshot}|null> {return invoke('library_snapshot_if_changed',{known});}
export function inspectBook(input: string): Promise<Catalog> { return invoke('inspect_book', { input }); }
export function downloadBook(input: string, first: number, last: number,selection?:string,destination?:SaveDestination): Promise<string> { return invoke('download_book', { input, first, last,selection:selection || null,destination:destination??null }); }
export function bookDetails(bookId:string):Promise<BookDetails> {return invoke('book_details',{bookId});}
export function readChapter(bookId:string,chapterId:string):Promise<ChapterContent> {return invoke('read_chapter',{bookId,chapterId});}
export function exportJob(jobId:string,format:'txt'|'epub'|'docx'):Promise<Receipt> {return invoke('export_job',{jobId,format});}
export function revealExport(id:string):Promise<void> {return invoke('reveal_export',{id});}
export function pauseJob(jobId:string):Promise<void> {return invoke('pause_job',{jobId});}
export function importBook(json: string,destination?:SaveDestination): Promise<string> { return invoke('import_book', { json,destination:destination??null }); }
export interface TxtSupplementPreview {fingerprint:string;title:string;total:number;already_saved:number;first_title:string;last_title:string}
export function previewTxtSupplement(bookId:string,text:string):Promise<TxtSupplementPreview> {return invoke('preview_txt_supplement',{bookId,text});}
export function confirmTxtSupplement(bookId:string,text:string,fingerprint:string):Promise<TxtSupplementPreview> {return invoke('confirm_txt_supplement',{bookId,text,fingerprint});}
export function resumeJob(jobId: string): Promise<void> { return invoke('resume_job', { jobId }); }
export function jobDetails(jobId:string):Promise<JobDetails> {return invoke('job_details',{jobId});}
export function retryChapters(jobId:string,chapters:string[]):Promise<void> {return invoke('retry_chapters',{jobId,chapters});}
export function updateBook(bookId:string):Promise<string> {return invoke('update_book',{bookId});}
export function cancelJob(jobId: string): Promise<void> { return invoke('cancel_job', { jobId }); }
export function exportBook(bookId: string, format: 'txt' | 'epub' | 'docx', selection?:string): Promise<Receipt> {
  return invoke('export_book', { bookId, format, selection:selection||null });
}

export interface AccountRecord {id:string;platform:string;label:string}
export interface AccountStatus {account:AccountRecord;session:'closed'|'open_unverified';blocked_navigation:boolean;can_download:boolean}
export function accountList():Promise<AccountStatus[]> {return invoke('account_list');}
export function createAccount(platform:string,label:string):Promise<AccountRecord> {return invoke('create_account',{platform,label});}
export function openAccount(id:string,bookUrl:string,login=false):Promise<void> {return invoke('open_account',{id,bookUrl,login});}
export function closeAccount(id:string,remove:boolean):Promise<void> {return invoke('close_account',{id,remove});}
export function focusAccount(id:string):Promise<void> {return invoke('focus_account',{id});}
export interface OfficialPage {platform:string;url:string;status:'text'|'excerpt'|'book'|'restricted'|'protected'|'verification'|'unsupported'|'loading';book_id:string;chapter_id:string;book_title:string;chapter_title:string;paragraphs:string[];declared_words:number|null;author?:string;description?:string}
export interface OfficialPreview {token:string;page:OfficialPage}
export interface OfficialExport {id:string;path:string}
export function openOfficialReader(platform:string,bookUrl:string,login:boolean):Promise<void>{return invoke('open_official_reader',{platform,bookUrl,login});}
export function inspectOfficialReader(platform:string):Promise<OfficialPreview>{return invoke('inspect_official_reader',{platform});}
export function saveOfficialReader(platform:string,token:string,acknowledged:boolean,directory?:string):Promise<OfficialExport>{return invoke('save_official_reader',{platform,token,acknowledged,directory:directory||null});}
export function chooseOfficialExportDirectory():Promise<string|null>{return invoke('choose_official_export_directory');}
export function revealOfficialReader(id:string):Promise<void>{return invoke('reveal_official_reader',{id});}
export function closeOfficialReader(platform:string):Promise<void>{return invoke('close_official_reader',{platform});}
export interface SessionChapter {remote_book_id:string;chapter:ChapterContent;declared_words:number}
export interface CapturePreview {token:string;kind:'catalog'|'chapter';catalog:Catalog|null;chapter:SessionChapter|null}
export interface CaptureReceipt {kind:'catalog'|'chapter';book_id:string;save:{book_id:string;title:string;chapter_title:string;position:number;saved:number;total:number;already_saved:boolean}|null}
export function previewAccountPage(id:string,kind:'catalog'|'chapter'):Promise<CapturePreview> {return invoke('preview_account_page',{id,kind});}
export function previewPlatformCatalog(id:string,bookUrl:string):Promise<CapturePreview> {return invoke('preview_platform_catalog',{id,bookUrl});}
export function confirmAccountPage(id:string,token:string):Promise<CaptureReceipt> {return invoke('confirm_account_page',{id,token});}
export function accountDownloadJobs(bookId:string):Promise<Job[]> {return invoke('account_download_jobs',{bookId});}
export function startAccountDownload(id:string,bookId:string,selection:string,noAutoPurchase:boolean,intervalMs=3000,destination?:SaveDestination):Promise<string> {return invoke('start_account_download',{id,bookId,selection:selection.trim()||null,noAutoPurchase,intervalMs,destination:destination??null});}
export function resumeAccountDownload(id:string,jobId:string,noAutoPurchase:boolean,intervalMs=3000):Promise<string> {return invoke('resume_account_download',{id,jobId,noAutoPurchase,intervalMs});}
