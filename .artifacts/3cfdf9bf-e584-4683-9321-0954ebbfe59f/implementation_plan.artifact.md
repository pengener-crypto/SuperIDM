# Fix Build Errors - Core, UI, and Engine

Resolving multiple compilation errors across the project, including incorrect API usages, missing imports, and syntax issues.

## Proposed Changes

### Core & Application
#### [MODIFY] [MainActivity.kt](file:///C:/Users/Haider%20Nasrat/Desktop/SuperIDM/app/src/main/java/com/superidm/MainActivity.kt)
- Fix `SettingsScreen` parameter: `onNavigateBack` -> `onBack` (matching the actual parameter name in `SettingsScreen.kt` if I decide to rename it, or vice versa. I will rename the parameter in `SettingsScreen.kt` to `onBack` for brevity).

### UI Layer
#### [MODIFY] [SettingsScreen.kt](file:///C:/Users/Haider%20Nasrat/Desktop/SuperIDM/app/src/main/java/com/superidm/ui/screens/SettingsScreen.kt)
- Rename `onNavigateBack` to `onBack` to align with common naming conventions and fix `MainActivity` call.

#### [MODIFY] [DownloadItemCard.kt](file:///C:/Users/Haider%20Nasrat/Desktop/SuperIDM/app/src/main/java/com/superidm/ui/components/DownloadItemCard.kt)
- Fix missing imports for `DownloadEntity`, `DownloadStatus`, `FormatUtils`, etc.

#### [MODIFY] [Type.kt](file:///C:/Users/Haider%20Nasrat/Desktop/SuperIDM/app/src/main/java/com/superidm/ui/theme/Type.kt)
- Fix unresolved `GoogleFont` and related imports by ensuring the correct `androidx.compose.ui.text.googlefonts` dependency and package.

### Engine & Network
#### [MODIFY] [Aria2RpcClient.kt](file:///C:/Users/Haider%20Nasrat/Desktop/SuperIDM/app/src/main/java/com/superidm/aria2/Aria2RpcClient.kt)
- Update `call` method to accept `KSerializer<T>` instead of `DeserializationStrategy<T>` to fix `RpcResponse.serializer()` call.

#### [MODIFY] [QueueManager.kt](file:///C:/Users/Haider%20Nasrat/Desktop/SuperIDM/app/src/main/java/com/superidm/engine/QueueManager.kt)
- Fix unresolved references to `DownloadRepository` methods and fix sorting logic.

#### [MODIFY] [DownloadEngine.kt](file:///C:/Users/Haider%20Nasrat/Desktop/SuperIDM/app/src/main/java/com/superidm/engine/DownloadEngine.kt)
- Fix coroutine scope issue in suspension function call.

### ViewModels
#### [MODIFY] [BrowserViewModel.kt](file:///C:/Users/Haider%20Nasrat/Desktop/SuperIDM/app/src/main/java/com/superidm/browser/BrowserViewModel.kt)
- Fix `DownloadEntity` constructor usage (parameter names and types).

## Verification Plan
### Automated Tests
- Run Gradle build (`:app:assembleDebug`) until all compilation errors are resolved.
