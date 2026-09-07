import { useCallback, useState, type ReactNode } from "react";
import { AppShell } from "./components/AppShell/AppShell";
import { AppRail, type NavTarget } from "./components/AppRail/AppRail";
import { LibraryEmptyState } from "./screens/Library/LibraryEmptyState";
import { Library } from "./screens/Library/Library";
import { ComingSoon } from "./screens/ComingSoon/ComingSoon";
import { Dashboard } from "./screens/Dashboard/Dashboard";
import { useChats } from "./hooks/useChats";
import { useImportChat } from "./hooks/useImportChat";
import { useDeleteChat } from "./hooks/useDeleteChat";
import { ImportFlowOverlay } from "./components/ImportFlow/ImportFlowOverlay";

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
  const { importFile, cancelImport, dismissError, isImporting, progress, error } = useImportChat({
    onImported: handleImported,
  });
  const { deleteChat } = useDeleteChat();

  function handleNavigate(target: NavTarget) {
    setOpenChatId(null);
    setActiveNav(target);
  }

  function handleSelectChat(id: number) {
    setOpenChatId(id);
  }

  async function handleDeleteChat(id: number) {
    await deleteChat(id);
    await refresh();
    setOpenChatId((current) => (current === id ? null : current));
  }

  let mainContent: ReactNode;
  if (openChatId != null) {
    mainContent = <Dashboard chatId={openChatId} />;
  } else if (activeNav === "library") {
    mainContent =
      chats.length > 0 ? (
        <Library
          chats={chats}
          onImport={importFile}
          isImporting={isImporting}
          error={error?.message ?? null}
          onSelectChat={handleSelectChat}
          onDelete={handleDeleteChat}
        />
      ) : (
        <LibraryEmptyState onPickFile={importFile} isImporting={isImporting} error={error?.message ?? null} />
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
      <ImportFlowOverlay
        progress={progress}
        error={error}
        onCancel={cancelImport}
        onPickAnother={importFile}
        onDismiss={dismissError}
      />
    </AppShell>
  );
}

export default App;
