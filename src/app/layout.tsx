import type { Metadata } from "next";
import { Roboto_Condensed } from "next/font/google";
import "./globals.css";

const robotoCondensed = Roboto_Condensed({
  variable: "--font-roboto-condensed",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "Safehand",
  description: "Safehand split cards",
  icons: {
    icon: "/logo.svg",
  },
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html
      lang="en"
      className={`${robotoCondensed.variable} h-full overflow-hidden antialiased`}
    >
      <body className={`${robotoCondensed.className} h-full overflow-hidden bg-background font-sans text-foreground`}>
        {children}
      </body>
    </html>
  );
}
