import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";
import "blobatar/motion.css";

const geist = Geist({
  variable: "--font-geist",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "Eunomia",
  description: "Shared long-term memory for your team's AI agents",
};

// Runs before paint so a saved light theme never flashes dark.
const THEME_SCRIPT = `try{var t=localStorage.getItem("eunomia-theme");if(t==="light")document.documentElement.dataset.theme="light"}catch(e){}`;

const DIRECTION_CONTRACT = `
THESIS: Eunomia is a multiplayer canvas: a team's agents and people write to one memory, and every memory shows who wrote it. Refuses the generic SaaS card-grid dashboard.
OWN-WORLD: Figma/tldraw grammar. Near-black (or white) ground with a faint dot grid, floating 1px hairline panels, small 7-10px radii, Geist + Geist Mono, felt green #3fa873 only on what you can press or have selected, author tags in a separate presence palette, square selection handles.
STORY: People see what their agents know, who added it and how fresh it is, then inspect, correct and govern it.
FIRST VIEWPORT: Slim sidebar left, dot-grid canvas, page title top left with the Cmd-K search, dense live table of memories with author tags, a floating inspector on the right for the selected item.
FORM: Multiplayer Canvas, grounded list position 5, seed key dcdcf93f.
FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
`;

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="en" className={`${geist.variable} ${geistMono.variable} h-full antialiased`} suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: THEME_SCRIPT }} />
      </head>
      <body className="min-h-full flex">
        <div hidden dangerouslySetInnerHTML={{ __html: `<!--${DIRECTION_CONTRACT}-->` }} />
        {children}
      </body>
    </html>
  );
}
