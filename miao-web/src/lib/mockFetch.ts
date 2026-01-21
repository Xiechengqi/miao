import { mockHandler } from "@/mocks";

// Store original fetch
const originalFetch = globalThis.fetch;

// Mock fetch implementation
async function mockFetch(
  input: RequestInfo | URL,
  init?: RequestInit
): Promise<Response> {
  const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  const method = (init?.method || "GET").toUpperCase() as "GET" | "POST" | "PUT" | "DELETE";

  // Only intercept /api/* requests
  if (url.includes("/api/")) {
    try {
      // Extract path from URL
      const urlObj = new URL(url, window.location.origin);
      const path = urlObj.pathname.replace(/^\/api\//, "");

      // Create a mock NextRequest-like object
      const mockRequest = {
        method,
        nextUrl: urlObj,
        json: async () => {
          if (init?.body) {
            return JSON.parse(init.body as string);
          }
          return {};
        },
      } as any;

      // Call mock handler
      const response = await mockHandler(method, path, mockRequest);

      // Convert NextResponse to standard Response
      const body = await response.text();
      return new Response(body, {
        status: response.status,
        statusText: response.statusText,
        headers: response.headers,
      });
    } catch (error) {
      console.error("Mock fetch error:", error);
      return new Response(
        JSON.stringify({ success: false, error: "Mock handler error" }),
        { status: 500, headers: { "Content-Type": "application/json" } }
      );
    }
  }

  // Pass through non-API requests
  return originalFetch(input, init);
}

// Install mock fetch
export function installMockFetch() {
  if (typeof window !== "undefined" && process.env.NEXT_PUBLIC_ENABLE_MOCK === "true") {
    console.log("🎭 Mock mode enabled - intercepting API calls");
    globalThis.fetch = mockFetch as any;
  }
}

// Restore original fetch
export function uninstallMockFetch() {
  if (typeof window !== "undefined") {
    globalThis.fetch = originalFetch;
  }
}
