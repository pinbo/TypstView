/**
 * Run an async job after edits settle. At most one run is in flight; requests
 * made meanwhile collapse into a single follow-up run.
 *
 * The delay is not restarted by later requests, so the job also keeps running
 * while someone types without pausing.
 */
export function coalesce(job: () => Promise<void>, delayMs: number): () => void {
  let timer: number | undefined;
  let running = false;
  let again = false;

  const run = async () => {
    timer = undefined;
    running = true;
    try {
      await job();
    } finally {
      running = false;
      if (again) {
        again = false;
        request();
      }
    }
  };

  const request = () => {
    if (running) again = true;
    else timer ??= window.setTimeout(run, delayMs);
  };

  return request;
}
