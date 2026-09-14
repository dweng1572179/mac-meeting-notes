export function createAutosave<T>(delay: number, save: (value: T) => Promise<void>) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending: { value: T } | undefined;
  let queue = Promise.resolve();

  async function saveValue(current: { value: T }) {
    try {
      await save(current.value);
    } catch (error) {
      pending ??= current;
      throw error;
    }
  }

  async function savePending() {
    const current = pending;
    if (!current) return;
    pending = undefined;
    await saveValue(current);
  }

  function enqueue(operationToRun: () => Promise<void>) {
    const operation = queue.then(operationToRun);
    queue = operation.catch(() => {});
    return operation;
  }

  function flush(): Promise<void> {
    clearTimeout(timer);
    timer = undefined;
    return enqueue(async () => {
      while (pending) {
        clearTimeout(timer);
        timer = undefined;
        await savePending();
      }
    });
  }

  return {
    schedule(value: T) {
      pending = { value };
      clearTimeout(timer);
      timer = setTimeout(() => {
        timer = undefined;
        const due = pending;
        if (!due) return;
        pending = undefined;
        void enqueue(() => saveValue(due)).catch(() => {});
      }, delay);
    },
    flush
  };
}
