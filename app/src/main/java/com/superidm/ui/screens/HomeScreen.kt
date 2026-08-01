package com.superidm.ui.screens

import android.os.Environment
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.superidm.browser.BrowserActivity
import com.superidm.data.db.DownloadEntity
import com.superidm.data.model.DownloadPriority
import com.superidm.data.model.DownloadStatus
import com.superidm.util.FileUtils
import com.superidm.util.FormatUtils
import com.superidm.viewmodel.HomeViewModel

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
    viewModel: HomeViewModel = hiltViewModel(),
    onNavigateToSettings: () -> Unit = {},
    onNavigateToHistory: () -> Unit = {},
    onNavigateToStreams: () -> Unit = {},
    onNavigateToTorrent: () -> Unit = {},
    onNavigateToAria2: () -> Unit = {},
    onNavigateToStatistics: () -> Unit = {}
) {
    val context = LocalContext.current
    var selectedTab by remember { mutableIntStateOf(0) }
    var speedDialExpanded by remember { mutableStateOf(false) }
    var showAddUrlDialog by remember { mutableStateOf(false) }

    val activeDownloads by viewModel.activeDownloads.collectAsState()
    val progressMap by viewModel.downloadProgress.collectAsState()
    val totalSpeed by viewModel.totalSpeed.collectAsState()
    val activeCount by viewModel.activeCount.collectAsState()

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text("SuperIDM", style = MaterialTheme.typography.titleLarge)
                        if (activeCount > 0 || totalSpeed > 0) {
                            Text(
                                text = "$activeCount active • ${FormatUtils.formatSpeed(totalSpeed)}",
                                style = MaterialTheme.typography.labelMedium,
                                color = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.8f)
                            )
                        }
                    }
                },
                actions = {
                    if (activeDownloads.isNotEmpty()) {
                        IconButton(onClick = { viewModel.pauseAll() }) {
                            Icon(Icons.Default.Pause, contentDescription = "Pause All")
                        }
                        IconButton(onClick = { viewModel.resumeAll() }) {
                            Icon(Icons.Default.PlayArrow, contentDescription = "Resume All")
                        }
                    }
                    IconButton(onClick = onNavigateToStatistics) {
                        Icon(Icons.Default.BarChart, contentDescription = "Statistics")
                    }
                    IconButton(onClick = onNavigateToAria2) {
                        Icon(Icons.Default.CloudDownload, contentDescription = "aria2c")
                    }
                    IconButton(onClick = onNavigateToSettings) {
                        Icon(Icons.Default.Settings, contentDescription = "Settings")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primaryContainer,
                    titleContentColor = MaterialTheme.colorScheme.onPrimaryContainer
                )
            )
        },
        bottomBar = {
            NavigationBar {
                NavigationBarItem(
                    selected = selectedTab == 0,
                    onClick = { selectedTab = 0 },
                    icon = { Icon(Icons.Default.Home, contentDescription = "Home") },
                    label = { Text("Home") }
                )
                NavigationBarItem(
                    selected = selectedTab == 1,
                    onClick = {
                        selectedTab = 1
                        onNavigateToHistory()
                    },
                    icon = { Icon(Icons.Default.History, contentDescription = "History") },
                    label = { Text("History") }
                )
                NavigationBarItem(
                    selected = selectedTab == 2,
                    onClick = {
                        selectedTab = 2
                        onNavigateToStreams()
                    },
                    icon = { Icon(Icons.Default.PlayCircleOutline, contentDescription = "Streams") },
                    label = { Text("Streams") }
                )
                NavigationBarItem(
                    selected = selectedTab == 3,
                    onClick = {
                        selectedTab = 3
                        onNavigateToSettings()
                    },
                    icon = { Icon(Icons.Default.Settings, contentDescription = "Settings") },
                    label = { Text("Settings") }
                )
            }
        },
        floatingActionButton = {
            Column(horizontalAlignment = Alignment.End) {
                AnimatedVisibility(visible = speedDialExpanded) {
                    Column(
                        horizontalAlignment = Alignment.End,
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier.padding(bottom = 16.dp)
                    ) {
                        ExtendedFloatingActionButton(
                            onClick = {
                                speedDialExpanded = false
                                showAddUrlDialog = true
                            },
                            icon = { Icon(Icons.Default.Link, contentDescription = null) },
                            text = { Text("Add URL") }
                        )
                        ExtendedFloatingActionButton(
                            onClick = {
                                speedDialExpanded = false
                                onNavigateToTorrent()
                            },
                            icon = { Icon(Icons.Default.InsertDriveFile, contentDescription = null) },
                            text = { Text("Add Torrent") }
                        )
                        ExtendedFloatingActionButton(
                            onClick = {
                                speedDialExpanded = false
                                BrowserActivity.start(context)
                            },
                            icon = { Icon(Icons.Default.Language, contentDescription = null) },
                            text = { Text("Open Browser") }
                        )
                        ExtendedFloatingActionButton(
                            onClick = {
                                speedDialExpanded = false
                                onNavigateToStreams()
                            },
                            icon = { Icon(Icons.Default.PlayCircle, contentDescription = null) },
                            text = { Text("Stream URL") }
                        )
                    }
                }
                FloatingActionButton(
                    onClick = { speedDialExpanded = !speedDialExpanded }
                ) {
                    Icon(
                        imageVector = if (speedDialExpanded) Icons.Default.Close else Icons.Default.Add,
                        contentDescription = "Speed Dial"
                    )
                }
            }
        }
    ) { padding ->
        Box(modifier = Modifier.fillMaxSize().padding(padding)) {
            if (activeDownloads.isEmpty()) {
                Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Icon(Icons.Default.Inbox, contentDescription = "Empty", modifier = Modifier.size(64.dp), tint = Color.Gray)
                        Spacer(modifier = Modifier.height(16.dp))
                        Text("No active downloads", color = Color.Gray, style = MaterialTheme.typography.titleMedium)
                        Spacer(modifier = Modifier.height(8.dp))
                        Button(onClick = { showAddUrlDialog = true }) {
                            Icon(Icons.Default.Add, contentDescription = null)
                            Spacer(modifier = Modifier.width(8.dp))
                            Text("Add Download")
                        }
                    }
                }
            } else {
                LazyColumn(
                    modifier = Modifier.fillMaxSize().padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    items(activeDownloads, key = { it.id }) { entity ->
                        val progress = progressMap[entity.id]
                        val downloadedBytes = progress?.downloadedBytes ?: entity.downloadedBytes
                        val totalBytes = progress?.totalBytes ?: entity.totalBytes
                        val speed = progress?.speed ?: entity.averageSpeed
                        val eta = progress?.eta ?: -1L
                        val status = progress?.status ?: entity.status

                        DownloadCard(
                            entity = entity,
                            downloadedBytes = downloadedBytes,
                            totalBytes = totalBytes,
                            speed = speed,
                            eta = eta,
                            status = status,
                            onPause = { viewModel.pauseDownload(entity.id) },
                            onResume = { viewModel.resumeDownload(entity.id) },
                            onCancel = { viewModel.cancelDownload(entity.id) }
                        )
                    }
                }
            }
        }

        if (showAddUrlDialog) {
            AddUrlDialog(
                onDismiss = { showAddUrlDialog = false },
                onAddDownload = { url, fileName, priority ->
                    val defaultDir = context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS)?.absolutePath
                        ?: context.filesDir.absolutePath
                    viewModel.addDownload(
                        url = url,
                        fileName = fileName.ifBlank { FileUtils.getFileNameFromUrl(url) },
                        saveDir = defaultDir,
                        priority = priority
                    )
                    showAddUrlDialog = false
                }
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddUrlDialog(
    onDismiss: () -> Unit,
    onAddDownload: (url: String, fileName: String, priority: DownloadPriority) -> Unit
) {
    val clipboardManager = LocalClipboardManager.current
    var urlText by remember { mutableStateOf("") }
    var fileNameText by remember { mutableStateOf("") }
    var selectedPriority by remember { mutableStateOf(DownloadPriority.NORMAL) }

    LaunchedEffect(Unit) {
        val clipText = clipboardManager.getText()?.text
        if (!clipText.isNullOrBlank() && (clipText.startsWith("http://") || clipText.startsWith("https://"))) {
            urlText = clipText
            fileNameText = FileUtils.getFileNameFromUrl(clipText)
        }
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Add New Download") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(
                    value = urlText,
                    onValueChange = {
                        urlText = it
                        if (fileNameText.isBlank()) {
                            fileNameText = FileUtils.getFileNameFromUrl(it)
                        }
                    },
                    label = { Text("Download URL") },
                    placeholder = { Text("https://example.com/file.zip") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    trailingIcon = {
                        IconButton(onClick = {
                            val clipText = clipboardManager.getText()?.text
                            if (!clipText.isNullOrBlank()) {
                                urlText = clipText
                                fileNameText = FileUtils.getFileNameFromUrl(clipText)
                            }
                        }) {
                            Icon(Icons.Default.ContentPaste, contentDescription = "Paste")
                        }
                    }
                )

                OutlinedTextField(
                    value = fileNameText,
                    onValueChange = { fileNameText = it },
                    label = { Text("File Name (Optional)") },
                    placeholder = { Text("file.zip") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth()
                )

                Text("Priority", style = MaterialTheme.typography.labelLarge)
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    DownloadPriority.entries.forEach { priority ->
                        FilterChip(
                            selected = selectedPriority == priority,
                            onClick = { selectedPriority = priority },
                            label = { Text(priority.label) }
                        )
                    }
                }
            }
        },
        confirmButton = {
            Button(
                enabled = urlText.isNotBlank(),
                onClick = { onAddDownload(urlText.trim(), fileNameText.trim(), selectedPriority) }
            ) {
                Text("Start Download")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel")
            }
        }
    )
}

@Composable
fun DownloadCard(
    entity: DownloadEntity,
    downloadedBytes: Long,
    totalBytes: Long,
    speed: Long,
    eta: Long,
    status: DownloadStatus,
    onPause: () -> Unit,
    onResume: () -> Unit,
    onCancel: () -> Unit
) {
    ElevatedCard(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    text = entity.fileName.ifEmpty { entity.url },
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f)
                )
                Spacer(modifier = Modifier.width(8.dp))
                AssistChip(
                    onClick = {},
                    label = { Text(status.name, style = MaterialTheme.typography.labelSmall) }
                )
            }

            Spacer(modifier = Modifier.height(8.dp))

            val progressFraction = FormatUtils.formatPercent(downloadedBytes, totalBytes)
            LinearProgressIndicator(
                progress = { progressFraction },
                modifier = Modifier.fillMaxWidth().height(8.dp)
            )

            Spacer(modifier = Modifier.height(8.dp))

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Text(
                    text = FormatUtils.formatProgress(downloadedBytes, totalBytes),
                    style = MaterialTheme.typography.bodySmall
                )
                if (status == DownloadStatus.DOWNLOADING) {
                    Text(
                        text = "${FormatUtils.formatSpeed(speed)} • ETA ${FormatUtils.formatEta(eta)}",
                        style = MaterialTheme.typography.bodySmall,
                        fontWeight = FontWeight.SemiBold
                    )
                }
            }

            Spacer(modifier = Modifier.height(8.dp))

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
                verticalAlignment = Alignment.CenterVertically
            ) {
                if (status == DownloadStatus.DOWNLOADING || status == DownloadStatus.CONNECTING) {
                    IconButton(onClick = onPause) {
                        Icon(Icons.Default.Pause, contentDescription = "Pause")
                    }
                } else if (status == DownloadStatus.PAUSED || status == DownloadStatus.QUEUED || status == DownloadStatus.FAILED) {
                    IconButton(onClick = onResume) {
                        Icon(Icons.Default.PlayArrow, contentDescription = "Resume")
                    }
                }
                IconButton(onClick = onCancel) {
                    Icon(Icons.Default.Close, contentDescription = "Cancel", tint = MaterialTheme.colorScheme.error)
                }
            }
        }
    }
}
