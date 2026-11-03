// Display-only: source paragraphs and exported text stay untouched.
export function readingParagraphs(paragraphs:readonly string[]):string[] {
  return paragraphs.flatMap(paragraph=>paragraph.split(/\r\n|[\r\n\u2028\u2029]/u))
    .map(line=>line.replace(/^[\t \u00a0\u3000]+/u,'').trimEnd())
    .filter(line=>line.length>0);
}
export function readingFontSize(value:unknown):number {
  const size=Number(value);
  return Number.isInteger(size)&&size>=16&&size<=28?size:20;
}
