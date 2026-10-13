export async function readSseText(
  body: ReadableStream<Uint8Array>,
  onText: (text: string) => void,
): Promise<void> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let pending = '';
  let content = '';
  let finished = false;

  const consume = (line: string) => {
    if (!line.startsWith('data:')) return;
    const data = line.slice(5).replace(/^ /, '').replace(/\r$/, '');
    if (data === '[DONE]') {
      finished = true;
      return;
    }
    if (data.startsWith('[ERROR]')) throw new Error(data.slice(7).trim());
    content += data.replace(/\\n/g, '\n');
    onText(content);
  };

  try {
    while (!finished) {
      const { done, value } = await reader.read();
      pending += done ? decoder.decode() : decoder.decode(value, { stream: true });
      const lines = pending.split('\n');
      pending = lines.pop() ?? '';
      for (const line of lines) {
        consume(line);
        if (finished) break;
      }
      if (done) {
        if (pending && !finished) consume(pending);
        break;
      }
    }
  } finally {
    await reader.cancel();
    reader.releaseLock();
  }
}
