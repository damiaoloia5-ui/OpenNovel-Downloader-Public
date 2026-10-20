import { invoke, isTauri } from '@tauri-apps/api/core';

export interface Book {
  id: string;
  title: string;
  author: string;
  source: string;
  saved_chapters: number;
  total_chapters: number;
}

export interface Job {
  id: string;
  book_id: string;
  title: string;
  state: string;
  saved: number;
  total: number;
  error: string | null;
}

export interface Snapshot { books: Book[]; jobs: Job[] }
export interface Receipt { path: string; saved_chapters: number; total_chapters: number; partial: boolean }

export const desktopAvailable = isTauri();

export function snapshot(): Promise<Snapshot> { return invoke('library_snapshot'); }
export function importBook(json: string): Promise<string> { return invoke('import_book', { json }); }
export function resumeJob(jobId: string): Promise<void> { return invoke('resume_job', { jobId }); }
export function cancelJob(jobId: string): Promise<void> { return invoke('cancel_job', { jobId }); }
export function exportBook(bookId: string, format: 'txt' | 'epub'): Promise<Receipt> {
  return invoke('export_book', { bookId, format });
}
