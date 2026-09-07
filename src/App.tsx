import { useState } from "react";
import { AppShell } from "./components/AppShell/AppShell";
import { AppRail, type NavTarget } from "./components/AppRail/AppRail";
import { LibraryEmptyState } from "./screens/Library/LibraryEmptyState";
import { ComingSoon } from "./screens/ComingSoon/ComingSoon";

const STUB_TITLES: Record<Exclude<NavTarget, "library">, string> = {
  people: "Участники",
  import: "Импорт",
  slides: "Слайды",
};

function App() {
  const [activeNav, setActiveNav] = useState<NavTarget>("library");

  return (
    <AppShell rail={<AppRail activeNav={activeNav} onNavigate={setActiveNav} />}>
      {activeNav === "library" ? <LibraryEmptyState /> : <ComingSoon title={STUB_TITLES[activeNav]} />}
    </AppShell>
  );
}

export default App;
