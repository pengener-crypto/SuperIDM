package com.superidm.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.superidm.aria2.Aria2DownloadStatus
import com.superidm.aria2.getFileName
import com.superidm.util.FormatUtils
import com.superidm.viewmodel.Aria2ViewModel

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun Aria2Screen(
    viewModel: Aria2ViewModel = hiltViewModel(),
    onNavigateToSettings: () -> Unit,
    onBack: () -> Unit
) {
    val isConnected by viewModel.isConnected.collectAsState()
    val globalStat by viewModel.globalStat.collectAsState()
    val activeDownloads by viewModel.activeDownloads.collectAsState()
    val waitingDownloads by viewModel.waitingDownloads.collectAsState()
    val completedDownloads by viewModel.completedDownloads.collectAsState()
    val urlToAdd by viewModel.urlToAdd.collectAsState()

    var selectedTab by remember { mutableIntStateOf(0) }
    val tabs = listOf("Active", "Waiting", "Completed")

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("aria2c") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    Badge(containerColor = if (isConnected) Color.Green else Color.Red) {
                        Text(if (isConnected) "ON" else "OFF", color = Color.White)
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
        }
    ) { padding ->
        if (!isConnected) {
            Column(
                modifier = Modifier.fillMaxSize().padding(padding),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center
            ) {
                Icon(Icons.Default.CloudOff, contentDescription = null, modifier = Modifier.size(64.dp), tint = MaterialTheme.colorScheme.error)
                Spacer(modifier = Modifier.height(16.dp))
                Text("Not connected to aria2c", style = MaterialTheme.typography.titleLarge)
                Spacer(modifier = Modifier.height(16.dp))
                Button(onClick = onNavigateToSettings) {
                    Text("Go to Settings")
                }
            }
        } else {
            Column(modifier = Modifier.fillMaxSize().padding(padding)) {
                // Global stats
                globalStat?.let { stat ->
                    Row(
                        modifier = Modifier.fillMaxWidth().background(Brush.horizontalGradient(listOf(MaterialTheme.colorScheme.primary.copy(alpha=0.1f), MaterialTheme.colorScheme.secondary.copy(alpha=0.1f)))).padding(8.dp),
                        horizontalArrangement = Arrangement.SpaceEvenly
                    ) {
                        Text("↓ \${FormatUtils.formatSpeed(stat.downloadSpeed)}")
                        Text("↑ \${FormatUtils.formatSpeed(stat.uploadSpeed)}")
                        Text("Active: \${stat.numActive}")
                        Text("Waiting: \${stat.numWaiting}")
                    }
                }

                // Add URL
                Row(modifier = Modifier.fillMaxWidth().padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(
                        value = urlToAdd,
                        onValueChange = { viewModel.urlToAdd.value = it },
                        modifier = Modifier.weight(1f),
                        placeholder = { Text("http://...") },
                        singleLine = true
                    )
                    Spacer(modifier = Modifier.width(8.dp))
                    Button(onClick = { viewModel.addUrl(urlToAdd, "") }) {
                        Text("Add")
                    }
                }

                TabRow(selectedTabIndex = selectedTab) {
                    tabs.forEachIndexed { index, title ->
                        Tab(
                            selected = selectedTab == index,
                            onClick = { selectedTab = index },
                            text = { 
                                val count = when(index) {
                                    0 -> activeDownloads.size
                                    1 -> waitingDownloads.size
                                    else -> completedDownloads.size
                                }
                                Text("\$title (\$count)") 
                            }
                        )
                    }
                }

                val currentList = when (selectedTab) {
                    0 -> activeDownloads
                    1 -> waitingDownloads
                    else -> completedDownloads
                }

                LazyColumn(modifier = Modifier.fillMaxSize(), contentPadding = PaddingValues(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    items(currentList, key = { it.gid }) { download ->
                        DownloadCard(
                            download = download,
                            onPause = { viewModel.pause(it) },
                            onResume = { viewModel.resume(it) },
                            onRemove = { viewModel.remove(it, false) }
                        )
                    }
                }
            }
        }
    }
}

@Composable
fun DownloadCard(
    download: Aria2DownloadStatus,
    onPause: (String) -> Unit,
    onResume: (String) -> Unit,
    onRemove: (String) -> Unit
) {
    ElevatedCard(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(12.dp)) {
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                Text(
                    text = download.getFileName(),
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f)
                )
                if (download.status == "complete") {
                    Icon(Icons.Default.CheckCircle, contentDescription = null, tint = Color.Green)
                } else if (download.status == "error") {
                    Icon(Icons.Default.Error, contentDescription = null, tint = Color.Red)
                }
            }
            
            Spacer(modifier = Modifier.height(8.dp))
            
            val total = download.totalLength.toLongOrNull() ?: 0L
            val completed = download.completedLength.toLongOrNull() ?: 0L
            val progress = if (total > 0) completed.toFloat() / total.toFloat() else 0f
            
            LinearProgressIndicator(
                progress = { progress },
                modifier = Modifier.fillMaxWidth(),
            )
            
            Spacer(modifier = Modifier.height(8.dp))
            
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                Column {
                    Text("\${FormatUtils.formatSize(completed)} / \${FormatUtils.formatSize(total)}", style = MaterialTheme.typography.bodySmall)
                    if (download.status == "active") {
                        Text("↓ \${FormatUtils.formatSpeed(download.downloadSpeed)}  ↑ \${FormatUtils.formatSpeed(download.uploadSpeed)}", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.primary)
                    }
                }
                
                Row {
                    if (download.status == "active") {
                        IconButton(onClick = { onPause(download.gid) }) { Icon(Icons.Default.Pause, contentDescription = "Pause") }
                    } else if (download.status == "paused" || download.status == "waiting") {
                        IconButton(onClick = { onResume(download.gid) }) { Icon(Icons.Default.PlayArrow, contentDescription = "Resume") }
                    }
                    IconButton(onClick = { onRemove(download.gid) }) { Icon(Icons.Default.Delete, contentDescription = "Remove") }
                }
            }
        }
    }
}


