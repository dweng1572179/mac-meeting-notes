export function createAutosave<T>(delay: number, save: (value: T) => Promise<void>) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending: { value: T } | undefined;
  let queue = Promise.resolve();

  async function savePending() {
    const current = pending;
    if (!current) return;
    pending = undefined;
    try {
      await save(current.value);
    } catch (error) {
      pending ??= current;
      throw error;
    }
  }

  function enqueue(drain: boolean) {
    const operation = queue.then(async () => {
      if (!drain) return savePending();
      while (pending) {
        clearTimeout(timer);
        timer = undefined;
        await savePending();
      }
    });
    queue = operation.catch(() => {});
    return operation;
  }

  function flush(): Promise<void> {
    clearTimeout(timer);
    timer = undefined;
    return enqueue(true);
  }

  return {
    schedule(value: T) {
      pending = { value };
      clearTimeout(timer);
      timer = setTimeout(() => {
        timer = undefined;
        void enqueue(false).catch(() => {});
      }, delay);
    },
    flush
  };
}
