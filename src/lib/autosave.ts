export function createAutosave<T>(delay: number, save: (value: T) => Promise<void>) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending: { value: T } | undefined;
  let saving: Promise<void> | undefined;

  async function drain() {
    try {
      while (pending) {
        const current = pending;
        pending = undefined;
        try {
          await save(current.value);
        } catch (error) {
          pending ??= current;
          throw error;
        }
      }
    } finally {
      saving = undefined;
    }
  }

  function flush() {
    clearTimeout(timer);
    timer = undefined;
    saving ??= pending ? drain() : undefined;
    return saving ?? Promise.resolve();
  }

  return {
    schedule(value: T) {
      pending = { value };
      clearTimeout(timer);
      timer = setTimeout(() => void flush().catch(() => {}), delay);
    },
    flush
  };
}
