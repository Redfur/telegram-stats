import { useCallback, useState, type ReactNode } from "react";
import { AppShell } from "./components/AppShell/AppShell";
import { AppRail, type NavTarget } from "./components/AppRail/AppRail";
import { LibraryEmptyState } from "./screens/Library/LibraryEmptyState";
import { ComingSoon } from "./screens/ComingSoon/ComingSoon";
import { Dashboard } from "./screens/Dashboard/Dashboard";
import { useChats } from "./hooks/useChats";
import { useImportChat } from "./hooks/useImportChat";

const STUB_TITLES: Record<Exclude<NavTarget, "library">, string> = {
  people: "Участники",
  import: "Импорт",
  slides: "Слайды",
};

function App() {
  const [activeNav, setActiveNav] = useState<NavTarget>("library");
  const [openChatId, setOpenChatId] = useState<number | null>(null);
  const { chats, refresh } = useChats();

  const handleImported = useCallback(
    async (chat: { id: number }) => {
      await refresh();
      setOpenChatId(chat.id);
    },
    [refresh],
  );
  const { importFile, isImporting, error } = useImportChat({ onImported: handleImported });

  function handleNavigate(target: NavTarget) {
    setOpenChatId(null);
    setActiveNav(target);
  }

  function handleSelectChat(id: number) {
    setOpenChatId(id);
  }

  let mainContent: ReactNode;
  if (openChatId != null) {
    mainContent = <Dashboard chatId={openChatId} />;
  } else if (activeNav === "library") {
    mainContent =
      chats.length > 0 ? (
        <ComingSoon title="Библиотека" message={`${chats.length} чат(ов) — откройте один из рельса`} />
      ) : (
        <LibraryEmptyState onPickFile={importFile} isImporting={isImporting} error={error} />
      );
  } else {
    mainContent = <ComingSoon title={STUB_TITLES[activeNav]} />;
  }

  return (
    <AppShell
      rail={
        <AppRail
          activeNav={activeNav}
          onNavigate={handleNavigate}
          chats={chats}
          activeChatId={openChatId}
          onSelectChat={handleSelectChat}
        />
      }
    >
      {mainContent}
    </AppShell>
  );
}

export default App;
