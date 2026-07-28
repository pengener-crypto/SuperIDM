package com.superidm.ui.screens

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowBack
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import com.superidm.ui.components.EmptyState
import com.superidm.util.FormatUtils
import com.superidm.viewmodel.HistoryViewModel
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HistoryScreen(
    viewModel: HistoryViewModel = hiltViewModel(),
    onBack: () -> Unit
) {
    val downloads by viewModel.filteredDownloads.collectAsState()
    val totalData by viewModel.totalDataDownloaded.collectAsState()
    val averageSpeed by viewModel.averageSpeed.collectAsState()
    val searchQuery by viewModel.searchQuery.collectAsState()

    var showClearDialog by remember { mutableStateOf(false) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("History") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.Default.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    IconButton(onClick = { showClearDialog = true }) {
                        Icon(Icons.Default.Delete, contentDescription = "Clear History")
                    }
                }
            )
        }
    ) { paddingValues ->
        Column(modifier = Modifier.padding(paddingValues).fillMaxSize()) {
            OutlinedTextField(
                value = searchQuery,
                onValueChange = { viewModel.search(it) },
                modifier = Modifier.fillMaxWidth().padding(16.dp),
                placeholder = { Text("Search downloads...") },
                leadingIcon = { Icon(Icons.Default.Search, contentDescription = null) },
                singleLine = true
            )

            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                horizontalArrangement = Arrangement.SpaceBetween
            ) {
                Text("Total Data: ${FormatUtils.formatSize(totalData)}", style = MaterialTheme.typography.bodyMedium)
                Text("Avg Speed: ${FormatUtils.formatSpeed(averageSpeed)}", style = MaterialTheme.typography.bodyMedium)
            }
            Spacer(modifier = Modifier.height(8.dp))

            if (downloads.isEmpty()) {
                EmptyState(
                    title = "No History",
                    subtitle = "Your completed downloads will appear here",
                    icon = Icons.Default.History,
                    modifier = Modifier.fillMaxSize()
                )
            } else {
                LazyColumn(modifier = Modifier.fillMaxSize()) {
                    items(downloads, key = { it.id }) { entity ->
                        val dateString = SimpleDateFormat("MMM dd, yyyy", Locale.getDefault()).format(Date(entity.completedAt))
                        ListItem(
                            headlineContent = { Text(entity.fileName.ifEmpty { entity.url }) },
                            supportingContent = { Text("$dateString • ${FormatUtils.formatSize(entity.totalBytes)}") },
                            trailingContent = {
                                IconButton(onClick = { viewModel.deleteDownload(entity.id) }) {
                                    Icon(Icons.Default.Delete, contentDescription = "Delete")
                                }
                            }
                        )
                        HorizontalDivider()
                    }
                }
            }
        }

        if (showClearDialog) {
            AlertDialog(
                onDismissRequest = { showClearDialog = false },
                title = { Text("Clear History?") },
                text = { Text("This will permanently remove all completed download records.") },
                confirmButton = {
                    TextButton(onClick = {
                        viewModel.clearHistory()
                        showClearDialog = false
                    }) {
                        Text("Clear")
                    }
                },
                dismissButton = {
                    TextButton(onClick = { showClearDialog = false }) {
                        Text("Cancel")
                    }
                }
            )
        }
    }
}
