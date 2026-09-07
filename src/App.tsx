import { useCallback, useState, type ReactNode } from "react";
import { AppShell } from "./components/AppShell/AppShell";
import { AppRail, type NavTarget } from "./components/AppRail/AppRail";
import { LibraryEmptyState } from "./screens/Library/LibraryEmptyState";
import { Library } from "./screens/Library/Library";
import { ComingSoon } from "./screens/ComingSoon/ComingSoon";
import { Dashboard } from "./screens/Dashboard/Dashboard";
import { Participants } from "./screens/Participants/Participants";
import { useChats } from "./hooks/useChats";
import { useImportChat } from "./hooks/useImportChat";
import { useDeleteChat } from "./hooks/useDeleteChat";
import { usePeopleReview } from "./hooks/usePeopleReview";
import { ImportFlowOverlay } from "./components/ImportFlow/ImportFlowOverlay";
import { ParticipantReviewStep } from "./components/ImportFlow/ParticipantReviewStep";

const STUB_TITLES: Record<Exclude<NavTarget, "library" | "people">, string> = {
  import: "Импорт",
  slides: "Слайды",
};

function App() {
  const [activeNav, setActiveNav] = useState<NavTarget>("library");
  const [openChatId, setOpenChatId] = useState<number | null>(null);
  const [reviewChatId, setReviewChatId] = useState<number | null>(null);
  const { chats, refresh } = useChats();

  const handleImported = useCallback(
    async (chat: { id: number }) => {
      await refresh();
      // Шаг 3 мастера импорта (макет 3d) — разбор участников — перед тем,
      // как открыть дашборд, а не сразу.
      setReviewChatId(chat.id);
    },
    [refresh],
  );
  const { importFile, cancelImport, dismissError, isImporting, progress, error } = useImportChat({
    onImported: handleImported,
  });
  const { deleteChat } = useDeleteChat();
  const { people, merge, hide, rename } = usePeopleReview(reviewChatId);

  function handleNavigate(target: NavTarget) {
    // «Участники» — экран для уже открытого чата (макет 3d/3e: чат в
    // рельсе и пункт нава подсвечены одновременно), закрывать чат при
    // переходе на него не нужно. Остальные пункты — самостоятельные
    // экраны, чат для них закрывается.
    if (target !== "people") {
      setOpenChatId(null);
    }
    setActiveNav(target);
  }

  function handleSelectChat(id: number) {
    setOpenChatId(id);
    // Клик по чату в рельсе всегда открывает его дашборд, даже если
    // до этого был открыт экран «Участники» — так остаётся понятный
    // путь назад к дашборду (повторный клик по тому же чату).
    setActiveNav("library");
  }

  async function handleDeleteChat(id: number) {
    await deleteChat(id);
    await refresh();
    setOpenChatId((current) => (current === id ? null : current));
    setReviewChatId((current) => (current === id ? null : current));
  }

  function finishReview() {
    if (reviewChatId != null) setOpenChatId(reviewChatId);
    setReviewChatId(null);
  }

  const openChat = openChatId != null ? chats.find((c) => c.id === openChatId) ?? null : null;

  let mainContent: ReactNode;
  if (activeNav === "people" && openChatId != null) {
    mainContent = <Participants chatId={openChatId} chatName={openChat?.name ?? null} />;
  } else if (openChatId != null) {
    mainContent = <Dashboard chatId={openChatId} onReimport={importFile} />;
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
  } else if (activeNav === "people") {
    mainContent = <ComingSoon title="Участники" message="Выберите чат слева, чтобы посмотреть участников" />;
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
      {reviewChatId != null && (
        <ParticipantReviewStep
          people={people}
          onMerge={merge}
          onHide={hide}
          onRename={rename}
          onDone={finishReview}
          onSkip={finishReview}
        />
      )}
    </AppShell>
  );
}

export default App;
