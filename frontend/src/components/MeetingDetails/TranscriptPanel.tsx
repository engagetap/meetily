"use client";

import { Transcript, TranscriptSegmentData } from '@/types';
import { TranscriptView } from '@/components/TranscriptView';
import { VirtualizedTranscriptView, InlineScreenshotData } from '@/components/VirtualizedTranscriptView';
import { TranscriptButtonGroup } from './TranscriptButtonGroup';
import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

type ScreenshotRow = {
  id: string;
  timestamp_ms: number;
  caption: string | null;
  accepted: number;
  image_path: string | null;
};

function useInlineScreenshots(
  meetingId?: string,
  meetingCreatedAtMs?: number,
): InlineScreenshotData[] {
  const [screenshots, setScreenshots] = useState<InlineScreenshotData[]>([]);

  useEffect(() => {
    if (!meetingId) return;
    let cancelled = false;
    (async () => {
      try {
        // Audio meetings and screen recordings have independent ids. Try the
        // direct match first (in case ids were unified at some point), then
        // fall back to timestamp-proximity resolution against the audio
        // meeting's created_at — same approach as ScreenshotsPanel.
        let list = await invoke<ScreenshotRow[]>('screenshots_list', { meetingId });
        if (list.length === 0 && meetingCreatedAtMs != null) {
          const screenId = await invoke<string | null>(
            'screenshots_resolve_recording_meeting_id',
            { nearMs: meetingCreatedAtMs, toleranceMs: 5 * 60 * 1000 },
          );
          if (screenId) {
            list = await invoke<ScreenshotRow[]>('screenshots_list', {
              meetingId: screenId,
            });
          }
        }

        const accepted = list.filter((s) => s.accepted === 1 && s.image_path);
        const out: InlineScreenshotData[] = [];
        for (const s of accepted) {
          try {
            const dataUrl = await invoke<string>('screenshots_read_image', { id: s.id });
            out.push({
              id: s.id,
              timestamp: s.timestamp_ms / 1000, // align with transcript seconds
              caption: s.caption,
              dataUrl,
            });
          } catch {
            // missing on disk; skip
          }
        }
        if (!cancelled) setScreenshots(out);
      } catch {
        // command might not exist (older builds); fail silent
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [meetingId, meetingCreatedAtMs]);

  return screenshots;
}

interface TranscriptPanelProps {
  transcripts: Transcript[];
  customPrompt: string;
  onPromptChange: (value: string) => void;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  isRecording: boolean;
  disableAutoScroll?: boolean;

  // Optional pagination props (when using virtualization)
  usePagination?: boolean;
  segments?: TranscriptSegmentData[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;

  // Retranscription props
  meetingId?: string;
  meetingCreatedAtMs?: number;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
}

export function TranscriptPanel({
  transcripts,
  customPrompt,
  onPromptChange,
  onCopyTranscript,
  onOpenMeetingFolder,
  isRecording,
  disableAutoScroll = false,
  usePagination = false,
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
  meetingId,
  meetingCreatedAtMs,
  meetingFolderPath,
  onRefetchTranscripts,
}: TranscriptPanelProps) {
  // Phase 3: fetch accepted screenshots for inline embedding.
  const inlineScreenshots = useInlineScreenshots(meetingId, meetingCreatedAtMs);

  // Convert transcripts to segments if pagination is not used but we want virtualization
  const convertedSegments = useMemo(() => {
    if (usePagination && segments) {
      return segments;
    }
    // Convert transcripts to segments for virtualization
    return transcripts.map(t => ({
      id: t.id,
      timestamp: t.audio_start_time ?? 0,
      endTime: t.audio_end_time,
      text: t.text,
      confidence: t.confidence,
    }));
  }, [transcripts, usePagination, segments]);

  return (
    <div className="hidden md:flex md:w-1/4 lg:w-1/3 min-w-0 border-r border-gray-200 bg-white flex-col relative shrink-0">
      {/* Title area */}
      <div className="p-4 border-b border-gray-200">
        <TranscriptButtonGroup
          transcriptCount={usePagination ? (totalCount ?? convertedSegments.length) : (transcripts?.length || 0)}
          onCopyTranscript={onCopyTranscript}
          onOpenMeetingFolder={onOpenMeetingFolder}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onRefetchTranscripts={onRefetchTranscripts}
        />
      </div>

      {/* Transcript content - use virtualized view for better performance */}
      <div className="flex-1 overflow-hidden pb-4">
        <VirtualizedTranscriptView
          segments={convertedSegments}
          screenshots={inlineScreenshots}
          isRecording={isRecording}
          isPaused={false}
          isProcessing={false}
          isStopping={false}
          enableStreaming={false}
          showConfidence={true}
          disableAutoScroll={disableAutoScroll}
          hasMore={hasMore}
          isLoadingMore={isLoadingMore}
          totalCount={totalCount}
          loadedCount={loadedCount}
          onLoadMore={onLoadMore}
        />
      </div>

      {/* Custom prompt input at bottom of transcript section */}
      {!isRecording && convertedSegments.length > 0 && (
        <div className="p-1 border-t border-gray-200">
          <textarea
            placeholder="Add context for AI summary. For example people involved, meeting overview, objective etc..."
            className="w-full px-3 py-2 border border-gray-200 rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-blue-500 focus:border-blue-500 bg-white shadow-sm min-h-[80px] resize-y"
            value={customPrompt}
            onChange={(e) => onPromptChange(e.target.value)}
          />
        </div>
      )}
    </div>
  );
}
