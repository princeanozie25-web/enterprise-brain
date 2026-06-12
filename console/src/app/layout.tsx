import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Ask Brain Console",
  description:
    "Enterprise Brain governed retrieval console — demo identity mode",
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
