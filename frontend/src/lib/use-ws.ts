"use client";

import { useEffect, useRef } from "react";
import { WS_BASE, type WsEvent } from "@/lib/api";

/** Subscribes to the backend's live event feed for the lifetime of the component. */
export function useLiveEvents(onEvent: (event: WsEvent) => void) {
  const handlerRef = useRef(onEvent);

  useEffect(() => {
    handlerRef.current = onEvent;
  }, [onEvent]);

  useEffect(() => {
    let socket: WebSocket | null = null;
    let closedByUs = false;
    let retryDelay = 1000;

    function connect() {
      socket = new WebSocket(`${WS_BASE}/ws`);
      socket.onmessage = (ev) => {
        try {
          const parsed = JSON.parse(ev.data) as WsEvent;
          handlerRef.current(parsed);
        } catch {
          // ignore malformed frames
        }
      };
      socket.onclose = () => {
        if (closedByUs) return;
        setTimeout(connect, retryDelay);
        retryDelay = Math.min(retryDelay * 1.5, 10000);
      };
    }

    connect();
    return () => {
      closedByUs = true;
      socket?.close();
    };
  }, []);
}
