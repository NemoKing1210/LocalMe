import type { MessageKey } from '@/i18n';
import type { Attachment, AttachmentState } from '@/ipc';

/** The sentence shown beside an attachment's size for every state but `offered`. */
const LABELS: Record<Exclude<AttachmentState, 'offered'>, MessageKey | null> = {
  queued: 'chat.attachmentQueued',
  sending: 'chat.attachmentSending',
  receiving: 'chat.attachmentReceiving',
  complete: null,
  cancelled: 'chat.attachmentCancelled',
  failed: 'chat.attachmentFailed',
};

/**
 * What the state of an attachment means to the person reading it, or `null` when there is nothing
 * to say — a file that is simply here.
 *
 * `offered` reads differently on the two sides: an incoming file waits for this device to ask for
 * it, an outgoing one waits for the recipient to.
 */
export function attachmentStateLabel(attachment: Attachment): MessageKey | null {
  if (attachment.state === 'offered') {
    return attachment.direction === 'incoming'
      ? 'chat.attachmentNotDownloaded'
      : 'chat.attachmentAwaitingDownload';
  }
  return LABELS[attachment.state];
}
