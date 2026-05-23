import { apiFetch } from "@/api/client";

export interface DeliveryItem {
  name: string;
  quantity?: number | null;
}

export interface Delivery {
  id: string;
  account_id: string;
  merchant?: string | null;
  carrier?: string | null;
  tracking_number?: string | null;
  tracking_url?: string | null;
  order_number?: string | null;
  status: string;
  eta_from?: string | null;
  eta_until?: string | null;
  delivered_at?: string | null;
  items: DeliveryItem[];
  confidence: number;
  source: string;
  thread_id?: string | null;
  last_event_at: string;
  created_at: string;
  updated_at: string;
  resolved_at?: string | null;
  dismissed_at?: string | null;
  message_ids: string[];
}

export type DeliveryFilter = "active" | "delivered" | "all" | "dismissed";

export function fetchDeliveries(filter: DeliveryFilter) {
  return apiFetch<{ deliveries: Delivery[] }>(
    `/api/v1/mail/deliveries?filter=${filter}`,
  );
}

export function resolveDelivery(id: string) {
  return apiFetch<unknown>(
    `/api/v1/mail/deliveries/${encodeURIComponent(id)}/resolve`,
    { method: "POST" },
  );
}

export function dismissDelivery(id: string) {
  return apiFetch<unknown>(
    `/api/v1/mail/deliveries/${encodeURIComponent(id)}/dismiss`,
    { method: "POST" },
  );
}
