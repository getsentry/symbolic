export function callsSyncCallback(cb) {
  cb();
}

export async function callsAsyncCallback(cb) {
  await cb();
}
