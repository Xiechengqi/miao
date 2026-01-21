import { NextRequest, NextResponse } from "next/server";
import { mockHandler } from "@/mocks/handlers";

export const dynamic = "force-static";
export const revalidate = 0;

const API_BASE = process.env.API_URL || "http://127.0.0.1:8080";
const ENABLE_MOCK = process.env.ENABLE_MOCK === "true";

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  const pathString = path.join("/");

  // Mock mode
  if (ENABLE_MOCK) {
    return mockHandler("GET", pathString, request);
  }

  const searchParams = request.nextUrl.searchParams.toString();
  const url = `${API_BASE}/${pathString}${searchParams ? `?${searchParams}` : ""}`;

  try {
    const headers: Record<string, string> = {};
    const authHeader = request.headers.get("Authorization");
    if (authHeader) {
      headers["Authorization"] = authHeader;
    }

    const response = await fetch(url, { headers });

    const data = await response.json().catch(() => ({}));

    return NextResponse.json(data, { status: response.status });
  } catch (error) {
    console.error("API proxy error:", error);
    return NextResponse.json(
      { success: false, error: "Backend unavailable" },
      { status: 503 }
    );
  }
}

export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  const pathString = path.join("/");

  // Mock mode
  if (ENABLE_MOCK) {
    return mockHandler("POST", pathString, request);
  }

  const url = `${API_BASE}/${pathString}`;

  try {
    const body = await request.json();

    const headers: Record<string, string> = {
      "Content-Type": "application/json",
    };
    const authHeader = request.headers.get("Authorization");
    if (authHeader) {
      headers["Authorization"] = authHeader;
    }

    const response = await fetch(url, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
    });

    const data = await response.json().catch(() => ({}));

    return NextResponse.json(data, { status: response.status });
  } catch (error) {
    console.error("API proxy error:", error);
    return NextResponse.json(
      { success: false, error: "Backend unavailable" },
      { status: 503 }
    );
  }
}

export async function PUT(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  const pathString = path.join("/");

  // Mock mode
  if (ENABLE_MOCK) {
    return mockHandler("PUT", pathString, request);
  }

  const url = `${API_BASE}/${pathString}`;

  try {
    const body = await request.json();

    const headers: Record<string, string> = {
      "Content-Type": "application/json",
    };
    const authHeader = request.headers.get("Authorization");
    if (authHeader) {
      headers["Authorization"] = authHeader;
    }

    const response = await fetch(url, {
      method: "PUT",
      headers,
      body: JSON.stringify(body),
    });

    const data = await response.json().catch(() => ({}));

    return NextResponse.json(data, { status: response.status });
  } catch (error) {
    console.error("API proxy error:", error);
    return NextResponse.json(
      { success: false, error: "Backend unavailable" },
      { status: 503 }
    );
  }
}

export async function DELETE(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  const pathString = path.join("/");

  // Mock mode
  if (ENABLE_MOCK) {
    return mockHandler("DELETE", pathString, request);
  }

  const url = `${API_BASE}/${pathString}`;

  try {
    const headers: Record<string, string> = {};
    const authHeader = request.headers.get("Authorization");
    if (authHeader) {
      headers["Authorization"] = authHeader;
    }

    const response = await fetch(url, {
      method: "DELETE",
      headers,
    });

    const data = await response.json().catch(() => ({}));

    return NextResponse.json(data, { status: response.status });
  } catch (error) {
    console.error("API proxy error:", error);
    return NextResponse.json(
      { success: false, error: "Backend unavailable" },
      { status: 503 }
    );
  }
}
