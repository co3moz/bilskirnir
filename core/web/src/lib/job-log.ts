/**
 * A job's log as the job drawer follows it: the first read brings the log the manager keeps, and
 * every later one only what was added since, from where the last read ended, so a running job's
 * whole log is not carried again every two seconds.
 */
import type { Job } from "./types"

/** What the manager keeps of a log, in bytes: `LOG_LIMIT` in core/manager/src/model.rs. */
export const LOG_LIMIT = 262_144

/**
 * A job as a read with `log_offset` answers it: `log_start` and `log_end` say where its log lies
 * among every byte the job ever wrote.
 */
export type JobRead = Job & { log_start?: number; log_end?: number }

/** The path that reads the job `id` with its log from where `before` ended, or all of it. */
export const jobReadPath = (id: string, before: JobRead | undefined) =>
  `jobs/${id}?log_offset=${before?.log_end ?? 0}`

const encoder = new TextEncoder()
const decoder = new TextDecoder()

/** The last `LOG_LIMIT` bytes of `log`, from the first character that starts in them. */
export function trimLog(log: string) {
  // No character takes more than three bytes per UTF-16 unit, so a short log is never measured.
  if (log.length * 3 <= LOG_LIMIT) return log
  const bytes = encoder.encode(log)
  if (bytes.length <= LOG_LIMIT) return log
  let at = bytes.length - LOG_LIMIT
  while (at < bytes.length && (bytes[at] & 0xc0) === 0x80) at++
  return decoder.decode(bytes.subarray(at))
}

/**
 * The job of `read`, which asked for its log from `before`'s end, with what it read joined to
 * `before`'s log and trimmed as the manager trims it. A part that does not start where `before`
 * ended, because there was no `before` or the bytes between were older than the log keeps, is
 * already the whole log the manager keeps.
 */
export function followLog(before: JobRead | undefined, read: JobRead): JobRead {
  if (before?.log_end === undefined || read.log_start !== before.log_end) return read
  return { ...read, log: read.log ? trimLog(before.log + read.log) : before.log }
}
