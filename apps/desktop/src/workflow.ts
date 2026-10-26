// UI routing only. The core still validates every official address and permission.
export function qidianBookLink(input: string): string | null {
  try {
    const url = new URL(input.trim());
    if (url.protocol !== 'https:' || url.hostname !== 'www.qidian.com' || url.port || url.username || url.password || url.search || url.hash) return null;
    const match = url.pathname.match(/^\/book\/(\d+)(?:\/catalog)?\/?$/);
    return match ? `https://www.qidian.com/book/${match[1]}/` : null;
  } catch { return null; }
}

export function qidianBookId(input: string): string | null {
  return qidianBookLink(input)?.split('/')[4] || null;
}
